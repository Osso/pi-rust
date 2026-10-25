use futures_util::FutureExt;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Map, Value, json};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
#[path = "extensions.rs"]
mod fixture;
#[path = "oracle.rs"]
mod oracle;
#[path = "ui_thread.rs"]
mod ui_thread;

#[derive(Default)]
struct Ui {
    calls: Arc<Mutex<Vec<Value>>>,
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
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let item = self.item.lock().unwrap().clone();
        let calls = self.calls.clone();
        ui_thread::run(move || {
            Box::pin(async move {
                pi_coding_agent::modes::interactive::theme::set_theme_instance(fixture_theme());
                let done_value = Arc::new(Mutex::new(None));
                let value = done_value.clone();
                let done = ExtensionUiDone::new(Rc::new(move |v| {
                    let mut value = value.lock().unwrap();
                    if value.is_none() {
                        *value = Some(v);
                    }
                }));
                let component = factory(
                    ExtensionUiFactoryContext {
                        tui: ExtensionUiTui::new(Arc::new(FixtureTui)),
                        theme: fixture_theme(),
                        keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(
                            Default::default(),
                            None,
                        ),
                    },
                    done,
                )
                .expect("custom component fixture factory")
                .now_or_never()
                .expect("fixture factory must resolve immediately")
                .expect("custom component fixture factory");
                let width = item["width"].as_u64().unwrap_or(40) as usize;
                calls.lock().unwrap().push(json!(["render", component.render(width)]));
                for key in item["keys"].as_array().into_iter().flatten() {
                    component.handle_input(&JsString::from(key.as_str().unwrap()));
                    calls.lock().unwrap().push(json!(["render", component.render(width)]));
                }
                component.invalidate();
                calls.lock().unwrap().push(json!(["render", component.render(width)]));
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
        })
    }
    fn set_editor_text(&self, text: &JsString) -> Result<(), pi_ai::request::CallbackError> {
        self.calls.lock().unwrap().push(json!(["editorText", text]));
        Ok(())
    }
    fn notify(
        &self,
        message: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.calls.lock().unwrap().push(json!(["notify", message, level]));
        Ok(())
    }
    fn select<'a>(
        &'a self,
        title: &JsString,
        items: &[JsString],
        _: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.calls.lock().unwrap().push(json!(["select", title, items]));
        let selected = self.selected.lock().unwrap().clone();
        Box::pin(async move { Ok(selected.map(JsString::from)) })
    }
    fn set_widget(
        &self,
        key: &JsString,
        content: Option<&[JsString]>,
        options: Option<&ExtensionWidgetOptions>,
    ) -> Result<(), pi_ai::request::CallbackError> {
        self.calls
            .lock()
            .unwrap()
            .push(json!(["widget", key, content, options]));
        Ok(())
    }
    fn set_working_message(&self, message: Option<&JsString>) {
        self.calls.lock().unwrap().push(json!(["workingMessage", message]));
    }
    fn set_working_indicator(&self, options: Option<&WorkingIndicatorOptions>) {
        self.calls.lock().unwrap().push(json!(["workingIndicator", options]));
    }
}

pub async fn compare(name: &str, factory: ExtensionFactory, mut request: Value) -> Value {
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
    let keys = request["keybindings"]
        .as_object()
        .map(|bindings| {
            bindings
                .iter()
                .map(|(id, keys)| (JsString::from(id), serde_json::from_value(keys.clone()).unwrap()))
                .collect()
        })
        .unwrap_or_default();
    pi_tui::keybindings::set_keybindings(pi_tui::keybindings::KeybindingsManager::new(
        pi_tui::keybindings::tui_keybindings(),
        keys,
    ));
    let cwd = request["cwd"].as_str().unwrap_or("/fixture");
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(name, factory);
    let extension = load_extension_from_factory(
        registry.get(&JsString::from(name)).unwrap().clone(),
        &JsString::from(cwd),
        fixture::event_bus(),
        runtime.clone(),
        &JsString::from(name),
    )
    .await
    .unwrap();
    let metadata = {
        let registration = extension.read();
        json!({"commands":registration.commands.keys().collect::<Vec<_>>(),"tools":registration.tools.keys().collect::<Vec<_>>()})
    };
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(Map::new())).unwrap();
    let model_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let model = if request["modelFixture"].is_object() {
        Some(configure_faux(
            &model_registry,
            &request["modelFixture"],
            model_calls.clone(),
        ))
    } else {
        None
    };
    let core_session = fixture::session_manager(cwd);
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd,
        core_session.clone(),
        Arc::new(model_registry),
        None,
    );
    let effects = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut actions = fixture::actions();
    if let Some(tools) = request["allTools"].as_array() {
        let infos:Vec<ToolInfo>=tools.iter().map(|name|serde_json::from_value(json!({"name":name,"description":"fixture tool","parameters":{},"sourceInfo":{"path":"/fixture","source":"fixture","scope":"temporary","origin":"top-level"}})).unwrap()).collect();
        actions.get_all_tools = Arc::new(move || Ok(infos.clone()));
        let active: Vec<String> = serde_json::from_value(request["activeTools"].clone()).unwrap();
        actions.get_active_tools = Arc::new(move || Ok(active.clone()));
        let log = effects.clone();
        actions.set_active_tools = Arc::new(move |names| {
            log.lock().unwrap().push(json!(["setActiveTools", names]));
            Ok(())
        });
        let manager = core_session.clone();
        let log = effects.clone();
        actions.append_entry = Arc::new(move |kind, data| {
            manager
                .lock()
                .unwrap()
                .append_custom_entry(kind, data.clone())
                .map_err(|e| HandlerError::new(e.to_string()))?;
            log.lock().unwrap().push(json!(["appendEntry", kind, data]));
            Ok(())
        });
    }
    let mut context = fixture::context_actions();
    if request["hasModel"].as_bool().unwrap_or(true) {
        context.get_model = Arc::new(move || model.clone().flatten());
    }
    runner.bind_core(actions, context, None).unwrap();
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
        let mut result = match case["event"].as_str().unwrap() {
            "session_start" => {
                runner
                    .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
                        reason: SessionStartReason::Startup,
                        previous_session_file: None,
                    }))
                    .await;
                Value::Null
            }
            "append_custom" => {
                core_session
                    .clone()
                    .lock()
                    .unwrap()
                    .append_custom_entry(
                        &JsString::from(case["kind"].as_str().unwrap()),
                        case.get("data").cloned().map(JsValue::from),
                    )
                    .unwrap();
                Value::Null
            }
            "append" => {
                core_session
                    .clone()
                    .lock()
                    .unwrap()
                    .append_message(serde_json::from_value(case["message"].clone()).unwrap())
                    .unwrap();
                Value::Null
            }
            "branch" => {
                let manager = core_session.clone();
                let mut manager = manager.lock().unwrap();
                let id = manager.get_entries()[case["index"].as_u64().unwrap() as usize]
                    .id()
                    .unwrap()
                    .to_owned();
                manager.branch(&id).unwrap();
                Value::Null
            }
            "entries" => {
                let entries = core_session.clone().lock().unwrap().get_entries();
                json!(
                    entries
                        .iter()
                        .map(|e| {
                            let mut v = serde_json::to_value(e).unwrap();
                            for k in ["id", "parentId", "timestamp"] {
                                v.as_object_mut().unwrap().remove(k);
                            }
                            v
                        })
                        .collect::<Vec<_>>()
                )
            }
            "event" => {
                let mut event = serde_json::from_value(case["value"].clone()).unwrap();
                if let ExtensionEvent::Compaction(compaction) = &mut event {
                    let controller_signal = pi_js_compat::abort::AbortController::new();
                    let signal = controller_signal.signal();
                    if case["aborted"] == true {
                        controller_signal.abort(None);
                    }
                    compaction.signal = Some(signal);
                }
                let result = runner.emit(&mut event).await;
                json!(result.compaction())
            }
            "render_call" => {
                let tool = runner.get_tool_definition(case["toolName"].as_str().unwrap()).unwrap();
                let component = (tool.render_call.as_ref().unwrap())(
                    &JsValue::from(case["params"].clone()),
                    &fixture_theme(),
                    &render_context(case),
                )
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
                        &JsString::from(case["text"].as_str().unwrap()),
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
                    JsValue::from(case["params"].clone()),
                    None,
                    None,
                    Some(runner.create_context()),
                )
                .await
                {
                    Ok(result) => json!({"value":result}),
                    Err(error) => json!({"error":error.message}),
                }
            }
            other => panic!("unknown fixture event: {other}"),
        };
        if request["captureOutputFiles"] == true {
            capture_artifact(&mut result);
        }
        cases.push(json!({"result":result,"ui":ui.calls.lock().unwrap().clone()}));
    }
    let mut actual = json!({"metadata":metadata,"cases":cases,"errors":errors.lock().unwrap().clone()});
    if request.get("allTools").is_some() {
        actual["effects"] = json!(effects.lock().unwrap().clone());
    }
    if request["modelFixture"].is_object() {
        let calls = model_calls.lock().unwrap().clone();
        if let Some(timestamp) = calls
            .first()
            .and_then(|call| call["context"]["messages"][0]["timestamp"].as_i64())
        {
            request["nowMillis"] = json!(timestamp);
        }
        actual["modelCalls"] = json!(calls);
    }
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
        "extension-examples-l-todo-oracle.ts",
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
    fn show_overlay(
        &self,
        _component: Box<dyn pi_coding_agent::core::extensions::ExtensionUiComponent>,
        _options: Option<pi_coding_agent::core::extensions::ExtensionUiOverlayOptions>,
    ) -> Rc<dyn pi_coding_agent::core::extensions::ExtensionUiOverlayHandle> {
        panic!("this fixture does not mount nested overlays")
    }

    fn request_render_force(&self) {
        panic!("fixture does not force redraw")
    }
    fn stop(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not release terminal")
    }
    fn start(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not acquire terminal")
    }

    fn create_fixed_cell(&self) -> std::rc::Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("this component fixture must not construct an editor")
    }
    fn request_render(&self) {}
    fn terminal_rows(&self) -> usize {
        24
    }
    fn terminal_columns(&self) -> usize {
        40
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
        .map(|(i, role)| {
            (
                JsString::from(role),
                ColorValue::Text(JsString::from(format!("#{:02x}2020", 50 + i))),
            )
        })
        .collect::<Vec<_>>();
    Arc::new(Theme::new(&colors, &[], ColorMode::Truecolor, None, None).unwrap())
}
fn render_context(case: &Value) -> ToolRenderContext {
    ToolRenderContext {
        args: JsValue::from(case["params"].clone()),
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

fn capture_artifact(result: &mut Value) {
    let Some(path) = result["value"]["details"]["fullOutputPath"].as_str().map(str::to_owned) else {
        return;
    };
    let file = std::path::Path::new(&path);
    assert_eq!(file.file_name().unwrap(), "output.txt");
    assert!(
        file.parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("pi-rg-")
    );
    let output = std::fs::read_to_string(file).unwrap();
    result["value"]["details"]["fullOutputPath"] = json!("<temporary>/pi-rg/output.txt");
    let text = result["value"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .replace(&path, "<temporary>/pi-rg/output.txt");
    result["value"]["content"][0]["text"] = json!(text);
    result["outputFile"] = json!(output);
    std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
}

fn configure_faux(
    runtime: &ModelRegistry,
    fixture: &Value,
    calls: Arc<Mutex<Vec<Value>>>,
) -> Option<pi_ai::types::Model> {
    use pi_ai::providers::faux::*;
    let provider = fixture["provider"].as_str().unwrap_or("google");
    let id = fixture["id"].as_str().unwrap_or("gemini-2.5-flash");
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some(provider.into()),
        models: Some(if fixture["missing"] == true {
            vec![]
        } else {
            vec![FauxModelDefinition {
                id: id.into(),
                ..Default::default()
            }]
        }),
        tokens_per_second: Some(1_000_000.0),
        ..Default::default()
    });
    let model = faux.get_model(None);
    let content: Vec<pi_ai::types::AssistantContent> = serde_json::from_value(
        fixture
            .get("content")
            .cloned()
            .unwrap_or(json!([{ "type":"text","text":"fixture answer"}])),
    )
    .unwrap();
    let failure = fixture["error"].as_str().map(str::to_owned);
    let stop = serde_json::from_value(fixture.get("stopReason").cloned().unwrap_or(json!("stop"))).unwrap();
    let response: FauxResponseFactory = Arc::new(move |context, options, _, model| {
        let options = options.unwrap_or_default().stream;
        if let Some(session) = &options.session_id {
            assert!(
                pi_js_compat::js_regex::JsRegex::new(
                    r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
                )
                .unwrap()
                .is_match(session)
            );
        }
        calls.lock().unwrap().push(json!({"model":[model.provider,model.id],"context":context,"options":{"maxTokens":options.max_tokens.map(|n|n as u64),"cacheRetention":options.cache_retention,"sessionId":options.session_id.map(|_|"<uuid-v7>"),"aborted":options.request.signal.as_ref().is_some_and(|s|s.aborted())}}));
        let response = faux_assistant_message(
            FauxContent::Blocks(content.clone()),
            FauxAssistantMessageOptions {
                stop_reason: Some(stop),
                timestamp: Some(0.0),
                ..Default::default()
            },
        );
        let failure = failure.clone();
        Box::pin(async move {
            match failure {
                Some(message) => Err(pi_ai::request::CallbackError::new(message)),
                None => Ok(response),
            }
        })
    });
    faux.set_responses((0..20).map(|_| FauxResponseStep::Factory(response.clone())).collect());
    scripted_registry::register_scripted_provider(&runtime, faux.provider);
    model
}

#[path = "scripted_registry.rs"]
mod scripted_registry;
