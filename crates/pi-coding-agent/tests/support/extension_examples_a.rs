//! Batch A's real-runtime transcript fixture. No provider calls or live session files.
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::footer_data_provider::FooterDataProvider;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::session_manager::CustomEntry;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme};
use serde_json::{Map, Value, json};

#[path = "footer_thread.rs"]
mod footer_thread;
use crate::{extension_support as fixture, oracle};

pub type Trace = Arc<Mutex<Vec<Value>>>;

struct CaptureUi {
    trace: Trace,
    theme: Arc<Theme>,
    dialogs: Mutex<Vec<Value>>,
    footer_data: Option<FooterDataProvider>,
    footer: footer_thread::FooterThread,
}
struct Terminal(Trace);
impl ExtensionUiTuiBackend for Terminal {
    fn request_component_render(&self, _identity: usize) {}
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
    fn request_render(&self) {
        self.0.lock().unwrap().push(json!(["requestRender"]));
    }
    fn terminal_rows(&self) -> usize {
        40
    }
    fn terminal_columns(&self) -> usize {
        80
    }
}
impl ExtensionUiContext for CaptureUi {
    fn set_footer(&self, factory: Option<ExtensionUiFooterFactory>) {
        self.trace.lock().unwrap().push(json!(["setFooter", factory.is_some()]));
        self.footer.mount(factory.map(|factory| {
            (
                factory,
                ExtensionUiFooterContext {
                    tui: ExtensionUiTui::new(Arc::new(Terminal(self.trace.clone()))),
                    theme: self.theme.clone(),
                    footer_data: self.footer_data.clone().unwrap(),
                },
            )
        }));
    }
    fn notify(
        &self,
        message: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.trace.lock().unwrap().push(json!(["notify", message, level]));
        Ok(())
    }
    fn set_status(
        &self,
        key: &JsString,
        text: Option<pi_coding_agent::core::footer_data_provider::ExtensionStatus>,
    ) -> Result<(), pi_ai::request::CallbackError> {
        let text = text.map(|text| text.evaluate(None)).transpose()?.flatten();
        self.trace.lock().unwrap().push(json!(["status", key, text]));
        Ok(())
    }
    fn set_working_indicator(&self, options: Option<&WorkingIndicatorOptions>) {
        self.trace.lock().unwrap().push(json!(["indicator", options]));
    }
    fn set_hidden_thinking_label(&self, label: Option<&JsString>) {
        self.trace.lock().unwrap().push(json!(["thinkingLabel", label]));
    }
    fn set_title(&self, title: &JsString) -> Result<(), pi_ai::request::CallbackError> {
        self.trace.lock().unwrap().push(json!(["title", title]));
        Ok(())
    }
    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, pi_ai::request::CallbackError>> {
        self.trace.lock().unwrap().push(json!([
            "confirm",
            title,
            message,
            opts.and_then(|o| o.timeout).map(pi_js_compat::number::js_number_value),
            opts.is_some_and(|o| o.signal.is_some())
        ]));
        let answer = self.dialogs.lock().unwrap().remove(0);
        let signal = opts.and_then(|o| o.signal.clone());
        Box::pin(async move {
            if let Some(error) = answer.get("error").and_then(Value::as_str) {
                return Err(error.into());
            }
            if answer == "abort" {
                signal.unwrap().wait().await;
                return Ok(false);
            }
            Ok(answer.as_bool().unwrap())
        })
    }
    fn select<'a>(
        &'a self,
        title: &JsString,
        items: &[JsString],
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.trace.lock().unwrap().push(json!([
            "select",
            title,
            items,
            opts.and_then(|o| o.timeout).map(pi_js_compat::number::js_number_value)
        ]));
        let answer = self.dialogs.lock().unwrap().remove(0);
        Box::pin(async move {
            if let Some(error) = answer.get("error").and_then(Value::as_str) {
                return Err(error.into());
            }
            Ok(answer.as_str().map(JsString::from))
        })
    }
    fn theme(&self) -> Result<Arc<Theme>, pi_ai::request::CallbackError> {
        Ok(self.theme.clone())
    }
}

pub async fn compare(name: &str, factory: impl FnOnce(Trace) -> ExtensionFactory, mut request: Value) -> Value {
    let actual = run(factory, &request).await;
    // Replay only the observed nondeterministic clock values. Assert the native
    // generated timestamp's wall-clock bounds separately in example_bookmark.
    if name == "bookmark" {
        request["times"] = json!(
            actual["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|action| {
                    if action[0] != "setLabel" {
                        return None;
                    }
                    action[2].as_str()?.strip_prefix("bookmark-")?.parse::<u64>().ok()
                })
                .collect::<Vec<_>>()
        );
    }
    if name == "entry-renderer" || name == "message-renderer" {
        request["times"] = json!(
            actual["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|item| {
                    match item[0].as_str() {
                        Some("appendEntry") => item[2]["timestamp"].as_f64(),
                        Some("sendMessage") => item[1]["details"]["timestamp"].as_f64(),
                        _ => None,
                    }
                })
                .collect::<Vec<_>>()
        );
    }
    let expected = run_oracle(name, &request);
    assert_eq!(actual, expected, "{name}: {request}");
    actual
}

pub fn run_oracle(name: &str, request: &Value) -> Value {
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-a-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
    );
    oracle.request(request)
}

pub async fn run(factory: impl FnOnce(Trace) -> ExtensionFactory, request: &Value) -> Value {
    let trace: Trace = Arc::default();
    let factory = factory(trace.clone());
    let runtime = fixture::runtime();
    let bus = fixture::event_bus();
    let mut registry = ExtensionRegistry::new();
    registry.register("<example:batch-a>", factory);
    let cwd = request["cwd"].as_str().unwrap_or("/fixture");
    let extension = load_extension_from_factory(
        registry.get(&JsString::from("<example:batch-a>")).unwrap().clone(),
        &JsString::from(cwd),
        bus.clone(),
        runtime.clone(),
        &JsString::from("<example:batch-a>"),
    )
    .await
    .unwrap();
    let commands: Vec<_> = extension
        .read()
        .commands
        .values()
        .map(|command| {
            json!([
                command.name,
                command.description,
                command.get_argument_completions.is_some()
            ])
        })
        .collect();
    let tools: Vec<_> = extension
        .read()
        .tools
        .values()
        .map(|tool| {
            let tool = &tool.definition;
            if request["metadata"].as_bool() == Some(true) {
                json!([
                    tool.name,
                    tool.label,
                    tool.description,
                    tool.parameters,
                    tool.prompt_snippet,
                    tool.prompt_guidelines
                ])
            } else {
                json!([tool.name, tool.label, tool.description, tool.parameters])
            }
        })
        .collect();
    let manager = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from("/fixture")), None).unwrap(),
    ));
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(Map::new())).unwrap();
    let observed: Trace = Arc::default();
    let mut extensions = vec![extension.clone()];
    if request["observeBus"] == true {
        let log = observed.clone();
        let observer: ExtensionFactory = Arc::new(move |api| {
            let log = log.clone();
            let result = api.events.on(
                &JsString::from("my:notification"),
                Arc::new(move |data| {
                    log.lock().unwrap().push(json!(["my:notification", data]));
                    Ok(())
                }),
            );
            Box::pin(async move { result.map(|_| ()).map_err(HandlerError::new) })
        });
        extensions.push(
            load_extension_from_factory(
                observer,
                &JsString::from("/fixture"),
                bus.clone(),
                runtime.clone(),
                &JsString::from("<observer:batch-a>"),
            )
            .await
            .unwrap(),
        );
    }
    let mut runner = ExtensionRunner::new(
        extensions,
        runtime,
        cwd,
        manager.clone(),
        Arc::new(model_registry),
        None,
    );
    let prompt = Arc::new(Mutex::new(String::new()));
    let mut context = fixture::context_actions();
    let current_prompt = prompt.clone();
    context.get_system_prompt = Arc::new(move || JsString::from(current_prompt.lock().unwrap().clone()));
    let mut actions = fixture::actions();
    let session_name = Arc::new(Mutex::new(request["name"].as_str().map(JsString::from)));
    let fixture_name = session_name.clone();
    let name = session_name.clone();
    let failure = request["actionError"].as_str().map(str::to_owned);
    let log = trace.clone();
    let entries = manager.clone();
    actions.append_entry = Arc::new(move |kind, data| {
        entries.lock().unwrap().append_custom_entry(kind, data.clone()).unwrap();
        log.lock().unwrap().push(json!(["appendEntry", kind, data]));
        Ok(())
    });
    if !request["model"].is_null() {
        let model: pi_ai::types::Model = serde_json::from_value(request["model"].clone()).unwrap();
        context.get_model = Arc::new(move || Some(model.clone()));
    }
    let log = trace.clone();
    let name_failure = failure.clone();
    actions.set_session_name = Arc::new(move |value| {
        if let Some(error) = &name_failure {
            return Err(HandlerError::new(error));
        }
        *name.lock().unwrap() = Some(value.into());
        log.lock().unwrap().push(json!(["setSessionName", value]));
        Ok(())
    });
    actions.get_session_name = Arc::new(move || Ok(session_name.lock().unwrap().clone()));
    let entries = manager.clone();
    let log = trace.clone();
    actions.set_label = Arc::new(move |id, label| {
        if let Some(error) = &failure {
            return Err(HandlerError::new(error));
        }
        let mut entries = entries.lock().unwrap();
        let index = entries
            .get_entries()
            .iter()
            .position(|entry| entry.id() == Some(id))
            .unwrap();
        entries
            .append_label_change(id, label)
            .map_err(|error| HandlerError::new(error.to_string()))?;
        assert_eq!(entries.get_label(id), label);
        log.lock().unwrap().push(json!(["setLabel", index, label]));
        Ok(())
    });
    let messages = trace.clone();
    let message_sent = Arc::new(tokio::sync::Notify::new());
    let sent = message_sent.clone();
    let send_error = request["sendError"].as_str().map(str::to_owned);
    actions.send_message = Arc::new(move |message, options| {
        messages.lock().unwrap().push(json!(["sendMessage", message, options]));
        sent.notify_one();
        if let Some(error) = &send_error {
            return Err(HandlerError::new(error));
        }
        Ok(())
    });
    let idle = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let current_idle = idle.clone();
    context.is_idle = Arc::new(move || current_idle.load(std::sync::atomic::Ordering::SeqCst));
    let log = trace.clone();
    context.shutdown = Arc::new(move || log.lock().unwrap().push(json!(["shutdown"])));
    let active_tools = Arc::new(Mutex::new(Vec::<String>::new()));
    let active = active_tools.clone();
    actions.get_active_tools = Arc::new(move || Ok(active.lock().unwrap().clone()));
    let log = trace.clone();
    actions.set_active_tools = Arc::new(move |names| {
        *active_tools.lock().unwrap() = names.to_vec();
        log.lock().unwrap().push(json!(["setActiveTools", names]));
        Ok(())
    });
    let log = trace.clone();
    let send_failure = request["sendError"].as_str().map(str::to_owned);
    actions.send_user_message = Arc::new(move |content, options| {
        if let Some(error) = &send_failure {
            return Err(HandlerError::new(error));
        }
        log.lock().unwrap().push(json!(["sendUserMessage", content, options]));
        Ok(())
    });
    runner.bind_core(actions, context, None).unwrap();
    let log = trace.clone();
    runner.bind_command_context(Some(ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
        wait_for_idle: Arc::new(|| Box::pin(async {})),
        new_session: Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        fork: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        navigate_tree: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        switch_session: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        reload: Arc::new(move || {
            let log = log.clone();
            Box::pin(async move {
                log.lock().unwrap().push(json!(["reload"]));
                Ok(())
            })
        }),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
    }));
    let errors = trace.clone();
    runner.on_error(Arc::new(move |error| {
        errors.lock().unwrap().push(json!(["handlerError", error.error]));
    }));
    let colors = [
        ("dim", "#5a5b5c"),
        ("accent", "#50b4e6"),
        ("success", "#22cc66"),
        ("toolTitle", "#d4d4d4"),
        ("text", "#eeeeee"),
        ("muted", "#808080"),
        ("error", "#cc2222"),
        ("warning", "#ccaa22"),
    ];
    let theme = Arc::new(
        Theme::new(
            &colors
                .into_iter()
                .map(|(role, color)| (role.into(), ColorValue::Text(color.into())))
                .collect::<Vec<_>>(),
            &[("customMessageBg".into(), ColorValue::Text("#101820".into()))],
            ColorMode::Truecolor,
            None,
            None,
        )
        .unwrap(),
    );
    let footer_root = tempfile::tempdir().unwrap();
    let footer_data = (request["footer"].as_bool() == Some(true)).then(|| {
        if let Some(branch) = request["branch"].as_str() {
            std::fs::create_dir(footer_root.path().join(".git")).unwrap();
            std::fs::write(
                footer_root.path().join(".git/HEAD"),
                format!("ref: refs/heads/{branch}\n"),
            )
            .unwrap();
        }
        FooterDataProvider::new(JsString::from(
            (footer_root.path()).to_str().expect("fixture path must be UTF-8"),
        ))
    });
    let ui = Arc::new(CaptureUi {
        trace: trace.clone(),
        theme: theme.clone(),
        footer_data: footer_data.clone(),
        footer: footer_thread::FooterThread::default(),
        dialogs: Mutex::new(request["dialogs"].as_array().cloned().unwrap_or_default()),
    });
    runner.set_ui_context(
        Some(ui.clone()),
        if request["mode"] == "json" {
            ExtensionMode::Json
        } else {
            ExtensionMode::Tui
        },
    );
    if request["noUi"] == true {
        runner.set_ui_context(None, ExtensionMode::Json);
    }
    let trigger_events = request["triggerPath"].as_str().map(|path| {
        use notify::Watcher;
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            tx.send(event).unwrap();
        })
        .unwrap();
        if std::path::Path::new(path).exists() {
            watcher
                .watch(std::path::Path::new(path), notify::RecursiveMode::NonRecursive)
                .unwrap();
        }
        (watcher, rx)
    });
    let mut trigger_events = trigger_events;
    if let Some(path) = request["execReceipt"].as_str() {
        let _ = std::fs::remove_file(path);
    }
    for step in request["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "idle" => idle.store(step["value"].as_bool().unwrap(), std::sync::atomic::Ordering::SeqCst),
            "command" => {
                let command = runner.get_command(step["name"].as_str().unwrap()).unwrap();
                if let Err(error) = (command.handler)(
                    step["args"].as_str().unwrap_or("").into(),
                    runner.create_command_context(),
                )
                .await
                {
                    trace.lock().unwrap().push(json!(["error", error.to_string()]));
                }
            }
            "event" | "emit" => {
                let mut event: ExtensionEvent = serde_json::from_value(step["event"].clone()).unwrap();
                let result = runner.emit(&mut event).await;
                tokio::task::yield_now().await;
                if step["op"] == "emit" {
                    let value = match result {
                        EmitResult::None => Value::Null,
                        EmitResult::SessionBeforeSwitch(result) => json!(result),
                        EmitResult::SessionBeforeFork(result) => json!(result),
                        other => panic!("unexpected result {other:?}"),
                    };
                    trace.lock().unwrap().push(json!(["result", value]));
                }
            }
            "trigger" => {
                let path = request["triggerPath"].as_str().unwrap();
                let content = step["content"].as_str().unwrap();
                let previous_messages = trace.lock().unwrap().iter().filter(|v| v[0] == "sendMessage").count();
                // One external write event, not truncate + write (which can race
                // the watcher into processing the same content twice).
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
                assert!(file.metadata().unwrap().len() <= content.len() as u64);
                file.write_all(content.as_bytes()).unwrap();
                drop(file);
                let receiver = &mut trigger_events.as_mut().unwrap().1;
                tokio::time::timeout(std::time::Duration::from_secs(3), async {
                    loop {
                        tokio::select! {
                            event = receiver.recv() => { if !event.unwrap().unwrap().kind.is_modify() { continue; } }
                            () = message_sent.notified() => {}
                        }
                        let file = std::fs::read_to_string(path).unwrap();
                        let expects_clear =
                            !pi_js_compat::string::js_trim(content).is_empty() && request["sendError"].is_null();
                        let expects_message = !pi_js_compat::string::js_trim(content).is_empty();
                        let delivered =
                            trace.lock().unwrap().iter().filter(|v| v[0] == "sendMessage").count() > previous_messages;
                        if (!expects_message || delivered) && (!expects_clear || file.is_empty()) {
                            break;
                        }
                    }
                })
                .await
                .expect("trigger fixture timeout");
                trace
                    .lock()
                    .unwrap()
                    .push(json!(["file", std::fs::read_to_string(path).unwrap()]));
            }
            "receipt" => {
                let path = request["execReceipt"].as_str().unwrap().to_owned();
                use notify::Watcher;
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    let _ = tx.send(event);
                })
                .unwrap();
                watcher
                    .watch(
                        std::path::Path::new(&path).parent().unwrap(),
                        notify::RecursiveMode::NonRecursive,
                    )
                    .unwrap();
                let bytes = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                    loop {
                        if let Ok(bytes) = std::fs::read(&path)
                            && !bytes.is_empty()
                        {
                            break bytes;
                        }
                        rx.recv().await.unwrap().unwrap();
                    }
                })
                .await
                .expect("command fixture timeout");
                let args: Value = serde_json::from_slice(&bytes).unwrap();
                trace.lock().unwrap().push(json!(["exec", args]));
            }
            "name" => {
                *fixture_name.lock().unwrap() = step["value"].as_str().map(str::to_owned).map(JsString::from);
            }
            "advance" => {
                tokio::time::advance(std::time::Duration::from_millis(step["ms"].as_u64().unwrap())).await;
                tokio::task::yield_now().await;
            }

            "bus" => {
                assert!(
                    bus.emit(
                        &JsString::from(step["channel"].as_str().unwrap()),
                        JsValue::from(step["data"].clone())
                    )
                    .is_empty()
                );
            }
            "prompt" => {
                *prompt.lock().unwrap() = step["value"].as_str().unwrap().into();
            }
            "before" => {
                let result = runner
                    .emit_before_agent_start(
                        &JsString::from("task"),
                        None,
                        &JsString::from(step["value"].as_str().unwrap()),
                        Default::default(),
                    )
                    .await;
                trace.lock().unwrap().push(json!(["before", result]));
            }
            "append" => {
                manager
                    .lock()
                    .unwrap()
                    .append_message(serde_json::from_value(step["message"].clone()).unwrap())
                    .unwrap();
            }
            "label" => {
                let mut entries = manager.lock().unwrap();
                let id = entries.get_entries()[step["index"].as_u64().unwrap() as usize]
                    .id()
                    .unwrap()
                    .to_owned();
                entries
                    .append_label_change(&id, step["value"].as_str().map(JsString::from).as_ref())
                    .unwrap();
            }
            "tools" => {
                let tools: Vec<_> = runner.get_all_registered_tools().iter().map(|registered| {
                    let tool = &registered.definition;
                    json!({"name":tool.name,"label":tool.label,"description":tool.description,"parameters":tool.parameters,"promptSnippet":tool.prompt_snippet,"promptGuidelines":tool.prompt_guidelines})
                }).collect();
                trace.lock().unwrap().push(json!(["tools", tools]));
            }
            "renderEntry" => {
                let entry: CustomEntry = serde_json::from_value(step["entry"].clone()).unwrap();
                let renderer = runner.get_entry_renderer(&entry.custom_type).unwrap();
                let component = renderer(
                    &entry,
                    &EntryRenderOptions {
                        expanded: step["expanded"].as_bool().unwrap(),
                        session_id: "fixture".into(),
                        request_render: std::rc::Rc::new(|| Ok(false)),
                        register_cleanup: std::rc::Rc::new(|_| {}),
                    },
                    &theme,
                )
                .unwrap();
                let lines = component.borrow_mut().render(step["width"].as_u64().unwrap() as usize);
                trace.lock().unwrap().push(json!(["renderEntry", lines]));
            }
            "renderMessage" => {
                let message: CustomMessageValue = serde_json::from_value(step["message"].clone()).unwrap();
                let renderer = runner.get_message_renderer(&message.custom_type).unwrap();
                let component = renderer(
                    &message,
                    &MessageRenderOptions {
                        expanded: step["expanded"].as_bool().unwrap(),
                    },
                    &theme,
                )
                .unwrap();
                let lines = component.borrow_mut().render(step["width"].as_u64().unwrap() as usize);
                trace.lock().unwrap().push(json!(["renderMessage", lines]));
            }
            "renderTool" => {
                let tool = runner.get_tool_definition(step["name"].as_str().unwrap()).unwrap();
                let result = serde_json::from_value(step["result"].clone()).unwrap();
                let options = ToolRenderResultOptions {
                    expanded: true,
                    is_partial: false,
                };
                let context = ToolRenderContext {
                    args: pi_js_compat::js_json!({}),
                    tool_call_id: "fixture-call".into(),
                    invalidate: Arc::new(|| {}),
                    last_component: None,
                    call_component: None,
                    state: ToolRenderState::new(),
                    cwd: "/fixture".into(),
                    execution_started: true,
                    args_complete: true,
                    is_partial: false,
                    expanded: true,
                    show_images: false,
                    is_error: false,
                };
                let component = tool.render_result.as_ref().unwrap()(&result, &options, &theme, &context).unwrap();
                let lines = component.borrow_mut().render(step["width"].as_u64().unwrap() as usize);
                trace.lock().unwrap().push(json!(["renderTool", lines]));
            }
            "footer" => {
                let lines = ui.footer.render(step["width"].as_u64().unwrap() as usize);
                trace.lock().unwrap().push(json!(["footer", lines]));
            }
            "branchChange" => {
                let dir = footer_root.path().join(step["name"].as_str().unwrap());
                std::fs::create_dir_all(dir.join(".git")).unwrap();
                std::fs::write(
                    dir.join(".git/HEAD"),
                    format!("ref: refs/heads/{}\n", step["name"].as_str().unwrap()),
                )
                .unwrap();
                footer_data
                    .as_ref()
                    .unwrap()
                    .set_cwd(JsString::from(dir.to_str().expect("fixture path must be UTF-8")))
                    .unwrap();
            }
            "tool" => {
                let tool = runner.get_tool_definition(step["name"].as_str().unwrap()).unwrap();
                let result = (tool.execute)(
                    "fixture-call".into(),
                    JsValue::from(step["params"].clone()),
                    None,
                    if request["updates"].as_bool() == Some(true) {
                        let log = trace.clone();
                        Some(Arc::new(move |update| {
                            log.lock().unwrap().push(json!(["update", update]));
                            Ok(())
                        }))
                    } else {
                        None
                    },
                    Some(runner.create_context()),
                )
                .await;
                match result {
                    Ok(result) => trace.lock().unwrap().push(json!(["tool", result])),
                    Err(error) => trace.lock().unwrap().push(json!(["error", error.to_string()])),
                }
            }
            other => panic!("unknown step {other}"),
        }
    }
    if let Some(footer_data) = footer_data {
        footer_data.dispose();
    }
    let mut result = json!({"commands":commands, "tools":tools, "trace":trace.lock().unwrap().clone()});
    if request["observeBus"] == true {
        result["observed"] = json!(observed.lock().unwrap().clone());
    }
    result
}

pub fn factory(register: fn(&ExtensionApi) -> Result<(), HandlerError>) -> impl FnOnce(Trace) -> ExtensionFactory {
    move |_trace| {
        Arc::new(move |api| {
            let result = register(api);
            Box::pin(async move { result })
        })
    }
}
