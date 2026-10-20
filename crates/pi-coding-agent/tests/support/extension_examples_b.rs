use pi_ai::auth::types::CredentialStore;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};
#[path = "oracle.rs"]
mod oracle;
#[path = "extensions.rs"]
mod support;

#[derive(Default)]
struct Ui {
    calls: Mutex<Vec<Value>>,
    selected: Mutex<Option<String>>,
    item: Mutex<Value>,
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Arc<pi_coding_agent::modes::interactive::theme::Theme> {
        fixture_theme()
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<Value>, String>> {
        Box::pin(async move {
            let item = self.item.lock().unwrap().clone();
            let done_value = Arc::new(Mutex::new(None));
            let value = done_value.clone();
            let done = ExtensionUiDone::new(Arc::new(move |v| {
                *value.lock().unwrap() = Some(v);
            }));
            let component = factory(
                ExtensionUiFactoryContext {
                    tui: ExtensionUiTui::new(Arc::new(FixtureTui)),
                    theme: fixture_theme(),
                    keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(Default::default(), None),
                },
                done,
            );
            let width = item["width"].as_u64().unwrap_or(40) as usize;
            self.calls
                .lock()
                .unwrap()
                .push(json!(["render", component.render(width)]));
            for key in item["keys"].as_array().into_iter().flatten() {
                component.handle_input(key.as_str().unwrap());
                self.calls
                    .lock()
                    .unwrap()
                    .push(json!(["render", component.render(width)]));
            }
            component.invalidate();
            self.calls
                .lock()
                .unwrap()
                .push(json!(["render", component.render(width)]));
            for _ in 0..1000 {
                if done_value.lock().unwrap().is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            component.dispose();
            let result = done_value
                .lock()
                .unwrap()
                .take()
                .ok_or("custom fixture never completed")?;
            Ok(result)
        })
    }
    fn notify(&self, message: &str, level: Option<NotifyType>) {
        self.calls.lock().unwrap().push(json!(["notify", message, level]));
    }
    fn select<'a>(
        &'a self,
        title: &str,
        items: &[String],
        _: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        self.calls.lock().unwrap().push(json!(["select", title, items]));
        let selected = self.selected.lock().unwrap().clone();
        Box::pin(async move { Ok(selected) })
    }
    fn set_widget(&self, key: &str, content: Option<&[String]>, options: Option<&ExtensionWidgetOptions>) {
        self.calls
            .lock()
            .unwrap()
            .push(json!(["widget", key, content, options]));
    }
    fn set_working_message(&self, message: Option<&str>) {
        self.calls.lock().unwrap().push(json!(["workingMessage", message]));
    }
    fn set_working_indicator(&self, options: Option<&WorkingIndicatorOptions>) {
        self.calls.lock().unwrap().push(json!(["workingIndicator", options]));
    }
}

pub async fn compare(name: &str, factory: ExtensionFactory, request: Value) -> Value {
    let reset_files: Vec<_> = request["resetFiles"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|path| {
            let path = std::path::PathBuf::from(path.as_str().unwrap());
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => panic!("fixture read: {error}"),
            };
            (path, bytes)
        })
        .collect();
    let cwd = request["cwd"].as_str().unwrap_or("/fixture");
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(name, factory);
    let extension = load_extension_from_factory(
        registry.get(name).unwrap().clone(),
        cwd,
        support::event_bus(),
        runtime.clone(),
        name,
    )
    .await
    .unwrap();
    let metadata = json!({"commands":extension.commands.keys().collect::<Vec<_>>(),"tools":extension.tools.keys().collect::<Vec<_>>()});
    let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(Map::new())) as Arc<dyn CredentialStore>),
        models_path: Some(None),
        allow_model_network: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut runner = support::runner_for(
        vec![extension],
        cwd,
        runtime,
        Arc::new(ModelRegistry::new(model_runtime)),
    );
    support::bind(&mut runner);
    let errors = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_errors = errors.clone();
    let _subscription = runner.on_error(Arc::new(move |error| {
        captured_errors.lock().unwrap().push(json!([error.event, error.error]));
    }));
    let ui = Arc::new(Ui::default());
    let mut cases = Vec::new();
    for case in request["cases"].as_array().unwrap() {
        ui.calls.lock().unwrap().clear();
        *ui.item.lock().unwrap() = case.clone();
        *ui.selected.lock().unwrap() = case["selected"].as_str().map(str::to_owned);
        runner.set_ui_context(
            if case["hasUI"].as_bool().unwrap_or(true) {
                Some(ui.clone())
            } else {
                None
            },
            serde_json::from_value(case.get("mode").cloned().unwrap_or(json!("tui"))).unwrap(),
        );
        let result = match case["event"].as_str().unwrap() {
            "session_start" => {
                runner
                    .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
                        reason: SessionStartReason::Startup,
                        previous_session_file: None,
                    }))
                    .await;
                Value::Null
            }
            "append" => {
                runner
                    .create_context()
                    .session_manager()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .append_message(serde_json::from_value(case["message"].clone()).unwrap())
                    .unwrap();
                Value::Null
            }
            "branch" => {
                let manager = runner.create_context().session_manager().unwrap();
                let mut manager = manager.lock().unwrap();
                let id = manager.get_entries()[case["index"].as_u64().unwrap() as usize]
                    .id()
                    .unwrap()
                    .to_owned();
                manager.branch(&id).unwrap();
                Value::Null
            }
            "event" => {
                let mut event = serde_json::from_value(case["value"].clone()).unwrap();
                runner.emit(&mut event).await;
                Value::Null
            }
            "render_call" => {
                let tool = runner.get_tool_definition(case["toolName"].as_str().unwrap()).unwrap();
                let component =
                    (tool.render_call.as_ref().unwrap())(&case["params"], &fixture_theme(), &render_context(case))
                        .unwrap();
                let lines = component
                    .borrow_mut()
                    .render(case["width"].as_u64().unwrap_or(40) as usize);
                json!(lines)
            }
            "render_result" => {
                let tool = runner.get_tool_definition(case["toolName"].as_str().unwrap()).unwrap();
                let result = serde_json::from_value(case["result"].clone()).unwrap();
                let opts = ToolRenderResultOptions {
                    expanded: case["expanded"].as_bool().unwrap_or(false),
                    is_partial: case["isPartial"].as_bool().unwrap_or(false),
                };
                let component =
                    (tool.render_result.as_ref().unwrap())(&result, &opts, &fixture_theme(), &render_context(case))
                        .unwrap();
                let lines = component
                    .borrow_mut()
                    .render(case["width"].as_u64().unwrap_or(40) as usize);
                json!(lines)
            }
            "command" => {
                let command = runner.get_command(case["name"].as_str().unwrap()).unwrap();
                match (command.handler)(
                    case["args"].as_str().unwrap_or("").into(),
                    runner.create_command_context(),
                )
                .await
                {
                    Ok(()) => Value::Null,
                    Err(error) => json!({"error":error.message}),
                }
            }
            "tool_call" => {
                let mut event=serde_json::from_value::<ToolCallEvent>(json!({"toolName":case["toolName"],"toolCallId":"fixture-call","input":case["input"],"bypassPermissions":false})).unwrap();
                json!(runner.emit_tool_call(&mut event).await.unwrap())
            }
            "input" => json!(
                runner
                    .emit_input(
                        case["text"].as_str().unwrap(),
                        serde_json::from_value(case["images"].clone()).unwrap(),
                        serde_json::from_value(case["source"].clone()).unwrap(),
                        serde_json::from_value(case["streamingBehavior"].clone()).unwrap()
                    )
                    .await
            ),
            "tool_metadata" => {
                let tool = runner
                    .get_tool_definition(case["toolName"].as_str().unwrap_or("bash"))
                    .unwrap();
                json!({"name":tool.name,"label":tool.label,"description":tool.description,"parameters":tool.parameters,"promptSnippet":tool.prompt_snippet,"promptGuidelines":tool.prompt_guidelines})
            }
            "execute" => {
                let tool = runner
                    .get_tool_definition(case["toolName"].as_str().unwrap_or("bash"))
                    .unwrap();
                match (tool.execute)(
                    "fixture-call".into(),
                    case["params"].clone(),
                    None,
                    None,
                    runner.create_context(),
                )
                .await
                {
                    Ok(result) => json!({"value":result}),
                    Err(error) => json!({"error":error.message}),
                }
            }
            other => panic!("unknown fixture event: {other}"),
        };
        cases.push(json!({"result":result,"ui":ui.calls.lock().unwrap().clone()}));
    }
    let actual = json!({"metadata":metadata,"cases":cases,"errors":errors.lock().unwrap().clone()});
    for (path, bytes) in reset_files {
        match bytes {
            Some(bytes) => std::fs::write(path, bytes).unwrap(),
            None => {
                if path.exists() {
                    std::fs::remove_file(path).unwrap();
                }
            }
        }
    }
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "extension-examples-b-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
        &oracle_environment(&request),
    );
    let expected = oracle.request(&request);
    assert_eq!(actual, expected, "example {name}");
    actual
}

fn oracle_environment(request: &Value) -> Vec<(&'static str, std::ffi::OsString)> {
    let mut env = vec![("LANG", "en_US.UTF-8".into()), ("LC_ALL", "en_US.UTF-8".into())];
    if let Some(agent) = request["agentDir"].as_str() {
        env.push(("PI_CODING_AGENT_DIR", agent.into()));
    }
    env
}

struct FixtureTui;
impl ExtensionUiTuiBackend for FixtureTui {
    fn request_render(&self) {}
    fn terminal_rows(&self) -> usize {
        24
    }
    fn terminal_columns(&self) -> usize {
        40
    }
    fn mode(&self) -> pi_tui::tui::TuiMode {
        pi_tui::tui::TuiMode::Regular
    }
}
fn fixture_theme() -> Arc<pi_coding_agent::modes::interactive::theme::Theme> {
    use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme};
    let roles = [
        "accent",
        "dim",
        "muted",
        "text",
        "success",
        "error",
        "warning",
        "toolTitle",
        "borderMuted",
        "border",
        "selectedBg",
    ];
    let colors = roles
        .into_iter()
        .enumerate()
        .map(|(i, role)| (role.to_owned(), ColorValue::Text(format!("#{:02x}2020", 50 + i))))
        .collect::<Vec<_>>();
    Arc::new(Theme::new(&colors, &[], ColorMode::Truecolor, None, None).unwrap())
}
fn render_context(case: &Value) -> ToolRenderContext {
    ToolRenderContext {
        args: case["params"].clone(),
        tool_call_id: "fixture-call".into(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state: ToolRenderState::new(),
        cwd: "/fixture".into(),
        execution_started: true,
        args_complete: true,
        is_partial: false,
        expanded: false,
        show_images: false,
        is_error: false,
    }
}
