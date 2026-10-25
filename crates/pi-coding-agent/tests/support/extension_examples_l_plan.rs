//! Modes extend A/B's registry + real-runner pattern; shared base actions live in extensions.rs.
use futures_util::FutureExt;
use pi_coding_agent::core::{extensions::*, model_registry::ModelRegistry, session_manager::SessionManager};
use pi_coding_agent::modes::interactive::theme::{ColorMode, Theme, load_theme_from_path};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
use serde_json::{Map, Value, json};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
#[path = "extensions.rs"]
mod fixture;
#[path = "oracle.rs"]
mod oracle;
pub type Trace = Arc<Mutex<Vec<JsValue>>>;
struct Ui {
    trace: Trace,
    step: Value,
    theme: Arc<Theme>,
}
struct Backend;
impl ExtensionUiTuiBackend for Backend {
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
        40
    }
    fn terminal_columns(&self) -> usize {
        80
    }
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Arc<Theme> {
        self.theme.clone()
    }
    fn notify(
        &self,
        text: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.trace
            .lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["notify", text, level]));
        Ok(())
    }
    fn set_status(
        &self,
        key: &JsString,
        text: Option<pi_coding_agent::core::footer_data_provider::ExtensionStatus>,
    ) -> Result<(), pi_ai::request::CallbackError> {
        let text = text.map(|text| text.evaluate(None)).transpose()?.flatten();
        self.trace
            .lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["status", key, text]));
        Ok(())
    }
    fn set_widget(
        &self,
        key: &JsString,
        text: Option<&[JsString]>,
        options: Option<&ExtensionWidgetOptions>,
    ) -> Result<(), pi_ai::request::CallbackError> {
        self.trace
            .lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["widget", key, text, options]));
        Ok(())
    }
    fn select<'a>(
        &'a self,
        title: &JsString,
        items: &[JsString],
        _: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.trace
            .lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["select", title, items]));
        let selected = self.step["selected"].as_str().map(str::to_owned);
        Box::pin(async move { Ok(selected.map(JsString::from)) })
    }
    fn editor<'a>(
        &'a self,
        title: &JsString,
        prefill: Option<&JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.trace
            .lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["editor", title, prefill]));
        let result = self.step["editor"].as_str().map(str::to_owned);
        Box::pin(async move { Ok(result.map(JsString::from)) })
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let result = Arc::new(Mutex::new(None));
        let captured = result.clone();
        let context = ExtensionUiFactoryContext {
            tui: ExtensionUiTui::new(Arc::new(Backend)),
            theme: self.theme.clone(),
            keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None),
        };
        let component = factory(
            context,
            ExtensionUiDone::new(Rc::new(move |value| *captured.lock().unwrap() = value)),
        )
        .expect("custom component fixture factory")
        .now_or_never()
        .expect("fixture factory must resolve immediately")
        .expect("custom component fixture factory");
        self.trace
            .lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["custom", component.render(80)]));
        let keys = self.step["keys"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![json!("\u{1b}")]);
        for key in keys {
            component.handle_input(&JsString::from(key.as_str().unwrap()));
            if result.lock().unwrap().is_none() && key.as_str() != Some("\u{1b}") {
                self.trace
                    .lock()
                    .unwrap()
                    .push(pi_js_compat::js_json!(["customRender", component.render(80)]));
            }
        }
        let value = result.lock().unwrap().clone();
        Box::pin(async move { Ok(value) })
    }
}
pub async fn compare(
    name: &str,
    register: fn(&ExtensionApi, Trace) -> Result<(), HandlerError>,
    request: Value,
) -> JsValue {
    let cwd = request["cwd"].as_str().unwrap_or("/fixture");
    let trace: Trace = Arc::default();
    let factory_trace = trace.clone();
    let runtime = create_extension_runtime();
    let mut factories = ExtensionRegistry::new();
    factories.register(
        name,
        Arc::new(move |api| {
            let result = register(api, factory_trace.clone());
            Box::pin(async move { result })
        }),
    );
    let extension = load_extension_from_factory(
        factories.get(&JsString::from(name)).unwrap().clone(),
        &JsString::from(cwd),
        fixture::event_bus(),
        runtime.clone(),
        &JsString::from("<example:g>"),
    )
    .await
    .unwrap();
    let metadata = {
        let registration = extension.read();
        json!({"commands":registration.commands.values().map(|c|json!([c.name,c.description,c.get_argument_completions.is_some()])).collect::<Vec<_>>(),"flags":registration.flags.values().map(|f|json!([f.name,f.description,f.flag_type,f.default])).collect::<Vec<_>>(),"shortcuts":registration.shortcuts.values().map(|s|json!([s.shortcut,s.description])).collect::<Vec<_>>()})
    };
    let registry = Arc::new(
        ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(Map::new())).unwrap(),
    );
    if let Some(models) = request["models"].as_array() {
        registry
            .register_provider(
                "fixture",
                pi_coding_agent::core::model_registry::ProviderConfigInput {
                    name: Some("fixture".into()),
                    api: Some("openai-completions".into()),
                    base_url: Some("https://fixture.invalid/v1".into()),
                    api_key: Some("fixture-key".into()),
                    models: Some(serde_json::from_value(json!(models)).unwrap()),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let manager = Arc::new(Mutex::new(if let Some(dir) = request["sessionDir"].as_str() {
        SessionManager::create(&JsString::from(cwd), Some(&JsString::from(dir)), None).unwrap()
    } else {
        SessionManager::in_memory(Some(&JsString::from(cwd)), None).unwrap()
    }));
    let mut runner = ExtensionRunner::new(vec![extension.clone()], runtime, cwd, manager.clone(), registry, None);
    let tools = Arc::new(Mutex::new(
        serde_json::from_value::<Vec<String>>(
            request
                .get("tools")
                .cloned()
                .unwrap_or(json!(["read", "bash", "edit", "write", "custom"])),
        )
        .unwrap(),
    ));
    let thinking = Arc::new(Mutex::new(
        serde_json::from_value(request.get("thinking").cloned().unwrap_or(json!("off"))).unwrap(),
    ));
    let model = Arc::new(Mutex::new(
        serde_json::from_value(request.get("initialModel").cloned().unwrap_or(Value::Null)).unwrap(),
    ));
    let mut actions = fixture::actions();
    let mut context = fixture::context_actions();
    let state = tools.clone();
    actions.get_active_tools = Arc::new(move || Ok(state.lock().unwrap().clone()));
    let state = tools.clone();
    let log = trace.clone();
    actions.set_active_tools = Arc::new(move |value| {
        *state.lock().unwrap() = value.to_vec();
        log.lock().unwrap().push(pi_js_compat::js_json!(["tools", value]));
        Ok(())
    });
    actions.get_all_tools = Arc::new(|| {
        Ok([
            "read",
            "bash",
            "edit",
            "write",
            "grep",
            "find",
            "ls",
            "ask_questions",
            "custom",
        ]
        .map(|name| ToolInfo {
            name: name.into(),
            description: name.into(),
            parameters: json!({}),
            prompt_guidelines: None,
            source_info: serde_json::from_value(
                json!({"path":"<fixture>","source":"fixture","scope":"temporary","origin":"top-level"}),
            )
            .unwrap(),
        })
        .into())
    });
    let state = thinking.clone();
    actions.get_thinking_level = Arc::new(move || Ok(*state.lock().unwrap()));
    let state = thinking.clone();
    let log = trace.clone();
    actions.set_thinking_level = Arc::new(move |value| {
        *state.lock().unwrap() = value;
        log.lock().unwrap().push(pi_js_compat::js_json!(["thinking", value]));
        Ok(())
    });
    let state = model.clone();
    let log = trace.clone();
    let success = request["modelSuccess"] != false;
    actions.set_model = Arc::new(move |value| {
        log.lock().unwrap().push(pi_js_compat::js_json!(["model", value]));
        if success {
            *state.lock().unwrap() = Some(value);
        }
        Box::pin(async move { Ok(success) })
    });
    let state = model.clone();
    context.get_model = Arc::new(move || state.lock().unwrap().clone());
    let entries = manager.clone();
    let log = trace.clone();
    actions.append_entry = Arc::new(move |kind, data| {
        entries
            .lock()
            .unwrap()
            .append_custom_entry(kind, data.clone())
            .map_err(|e| HandlerError::new(e.to_string()))?;
        log.lock().unwrap().push(pi_js_compat::js_json!(["append", kind, data]));
        Ok(())
    });
    let log = trace.clone();
    actions.send_message = Arc::new(move |message, options| {
        log.lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["message", message, options]));
        Ok(())
    });
    let log = trace.clone();
    actions.send_user_message = Arc::new(move |message, options| {
        log.lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["user", message, options]));
        Ok(())
    });
    runner.bind_core(actions, context, None).unwrap();
    let log = trace.clone();
    let _errors = runner.on_error(Arc::new(move |error| {
        log.lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["handlerError", error.event, error.error]))
    }));
    for (key, value) in request["flags"].as_object().into_iter().flatten() {
        runner.set_flag_value(key, serde_json::from_value(value.clone()).unwrap());
    }
    for item in request["entries"].as_array().into_iter().flatten() {
        let mut m = manager.lock().unwrap();
        match item["type"].as_str().unwrap() {
            "custom" => {
                m.append_custom_entry(
                    &JsString::from(item["customType"].as_str().unwrap()),
                    item.get("data").cloned().map(JsValue::from),
                )
                .unwrap();
            }
            "custom_message" => {
                m.append_custom_message_entry(
                    &JsString::from(item["customType"].as_str().unwrap()),
                    serde_json::from_value(item["content"].clone()).unwrap(),
                    item["display"].as_bool().unwrap(),
                    item.get("details").cloned().map(JsValue::from),
                )
                .unwrap();
            }
            _ => {
                m.append_message(serde_json::from_value(item["message"].clone()).unwrap())
                    .unwrap();
            }
        }
    }
    let theme = Arc::new(
        load_theme_from_path(
            &JsString::from(
                (&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json"))
                    .to_str()
                    .expect("fixture path must be UTF-8"),
            ),
            Some(ColorMode::Truecolor),
        )
        .unwrap(),
    );
    for step in request["steps"].as_array().unwrap() {
        runner.set_ui_context(
            if step["hasUI"] == false {
                None
            } else {
                Some(Arc::new(Ui {
                    trace: trace.clone(),
                    step: step.clone(),
                    theme: theme.clone(),
                }))
            },
            ExtensionMode::Tui,
        );
        match step["op"].as_str().unwrap() {
            "command" => {
                let command = runner.get_command(step["name"].as_str().unwrap()).unwrap();
                if let Err(error) = (command.handler)(
                    step["args"].as_str().unwrap_or("").into(),
                    runner.create_command_context(),
                )
                .await
                {
                    trace
                        .lock()
                        .unwrap()
                        .push(pi_js_compat::js_json!(["error", error.message]));
                }
            }
            "shortcut" => {
                let handler = extension
                    .read()
                    .shortcuts
                    .values()
                    .find(|s| s.shortcut == step["key"].as_str().unwrap())
                    .unwrap()
                    .handler
                    .clone();
                handler(runner.create_context()).await.unwrap();
            }
            "event" => {
                runner
                    .emit(&mut serde_json::from_value(step["event"].clone()).unwrap())
                    .await;
            }
            "before" => {
                let value = runner
                    .emit_before_agent_start(
                        &JsString::from("task"),
                        None,
                        &JsString::from(step["prompt"].as_str().unwrap_or("base")),
                        Default::default(),
                    )
                    .await;
                trace.lock().unwrap().push(pi_js_compat::js_json!(["before", value]));
            }
            "context" => {
                let value = runner
                    .emit_context(serde_json::from_value(step["messages"].clone()).unwrap())
                    .await;
                trace
                    .lock()
                    .unwrap()
                    .push(pi_js_compat::js_json!(["context", value.unwrap()]));
            }
            "tool" => {
                let mut event=serde_json::from_value(json!({"toolName":step["name"],"toolCallId":"fixture","input":step["input"],"bypassPermissions":false})).unwrap();
                let value = runner.emit_tool_call(&mut event).await.unwrap();
                trace.lock().unwrap().push(pi_js_compat::js_json!(["tool", value]));
            }
            "tools" => *tools.lock().unwrap() = serde_json::from_value(step["value"].clone()).unwrap(),
            other => panic!("unknown op {other}"),
        }
    }
    let entries: Vec<_> = manager
        .lock()
        .unwrap()
        .get_entries()
        .iter()
        .map(|entry| {
            let mut entry = pi_js_compat::js_json::to_value(entry).unwrap();
            for key in ["id", "parentId", "timestamp"] {
                entry.as_object_mut().unwrap().shift_remove(key);
            }
            entry
        })
        .collect();
    if request["checkRawPersistence"] == true {
        let file = manager.lock().unwrap().get_session_file().unwrap().to_owned();
        let persisted = std::fs::read_to_string(&encode_node_utf8(&file)).unwrap();
        assert!(
            persisted.contains("\\ud83d..."),
            "persisted JSONL retains generated surrogate"
        );
        assert!(
            !persisted.contains("�..."),
            "native JSONL must not replace generated surrogate"
        );
    }
    let actual = pi_js_compat::js_json!({"metadata":metadata,"trace":trace.lock().unwrap().clone(),"tools":tools.lock().unwrap().clone(),"thinking":*thinking.lock().unwrap(),"model":model.lock().unwrap().clone(),"entries":entries});
    let module = if name == "plan-mode" { "plan-mode/index" } else { name };
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-l-plan-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{module}.ts"),
    );
    let expected = oracle.request(&JsValue::from(request.clone()));
    assert_eq!(actual["metadata"], expected["metadata"], "{name} metadata");
    for (index, entry) in actual["trace"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            *entry, expected["trace"][index],
            "{name} trace {index}; request {request}"
        );
    }
    assert_eq!(actual, expected, "{name}");
    actual
}
