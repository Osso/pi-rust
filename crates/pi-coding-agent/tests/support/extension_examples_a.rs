//! Batch A's real-runtime transcript fixture. No provider calls or live session files.
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::CredentialStore;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme};
use serde_json::{Map, Value, json};

#[path = "oracle.rs"]
mod oracle;
#[path = "extensions.rs"]
mod support;

pub type Trace = Arc<Mutex<Vec<Value>>>;

struct CaptureUi {
    trace: Trace,
    theme: Arc<Theme>,
    dialogs: Mutex<Vec<Value>>,
}
impl ExtensionUiContext for CaptureUi {
    fn notify(&self, message: &str, level: Option<NotifyType>) {
        self.trace.lock().unwrap().push(json!(["notify", message, level]));
    }
    fn set_status(&self, key: &str, text: Option<&str>) {
        self.trace.lock().unwrap().push(json!(["status", key, text]));
    }
    fn set_working_indicator(&self, options: Option<&WorkingIndicatorOptions>) {
        self.trace.lock().unwrap().push(json!(["indicator", options]));
    }
    fn set_hidden_thinking_label(&self, label: Option<&str>) {
        self.trace.lock().unwrap().push(json!(["thinkingLabel", label]));
    }
    fn set_title(&self, title: &str) {
        self.trace.lock().unwrap().push(json!(["title", title]));
    }
    fn confirm<'a>(
        &'a self,
        title: &str,
        message: &str,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, String>> {
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
                signal.unwrap().cancelled().await;
                return Ok(false);
            }
            Ok(answer.as_bool().unwrap())
        })
    }
    fn select<'a>(
        &'a self,
        title: &str,
        items: &[String],
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
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
            Ok(answer.as_str().map(str::to_owned))
        })
    }
    fn theme(&self) -> Arc<Theme> {
        self.theme.clone()
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
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-a-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
    );
    let expected = oracle.request(&request);
    assert_eq!(actual, expected, "{name}: {request}");
    actual
}

async fn run(factory: impl FnOnce(Trace) -> ExtensionFactory, request: &Value) -> Value {
    let trace: Trace = Arc::default();
    let factory = factory(trace.clone());
    let runtime = support::runtime();
    let bus = support::event_bus();
    let mut registry = ExtensionRegistry::new();
    registry.register("<example:batch-a>", factory);
    let cwd = request["cwd"].as_str().unwrap_or("/fixture");
    let extension = load_extension_from_factory(
        registry.get("<example:batch-a>").unwrap().clone(),
        cwd,
        bus.clone(),
        runtime.clone(),
        "<example:batch-a>",
    )
    .await
    .unwrap();
    let commands: Vec<_> = extension
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
        .tools
        .values()
        .map(|tool| {
            let tool = &tool.definition;
            json!([tool.name, tool.label, tool.description, tool.parameters])
        })
        .collect();
    let manager = Arc::new(Mutex::new(SessionManager::in_memory(Some("/fixture"), None).unwrap()));
    let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(Map::new())) as Arc<dyn CredentialStore>),
        models_path: Some(None),
        allow_model_network: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut runner = ExtensionRunner::new(
        vec![extension.clone()],
        runtime,
        cwd,
        manager.clone(),
        Arc::new(ModelRegistry::new(model_runtime)),
        None,
    );
    let prompt = Arc::new(Mutex::new(String::new()));
    let mut context = support::context_actions();
    let current_prompt = prompt.clone();
    context.get_system_prompt = Arc::new(move || current_prompt.lock().unwrap().clone());
    let mut actions = support::actions();
    let session_name = Arc::new(Mutex::new(request["name"].as_str().map(str::to_owned)));
    let name = session_name.clone();
    let log = trace.clone();
    let failure = request["actionError"].as_str().map(str::to_owned);
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
    runner.bind_core(actions, context, None);
    let errors = trace.clone();
    runner.on_error(Arc::new(move |error| {
        errors.lock().unwrap().push(json!(["handlerError", error.error]));
    }));
    let colors = [("dim", "#5a5b5c"), ("accent", "#50b4e6"), ("success", "#22cc66")];
    let theme = Arc::new(
        Theme::new(
            &colors
                .into_iter()
                .map(|(role, color)| (role.into(), ColorValue::Text(color.into())))
                .collect::<Vec<_>>(),
            &[],
            ColorMode::Truecolor,
            None,
            None,
        )
        .unwrap(),
    );
    runner.set_ui_context(
        Some(Arc::new(CaptureUi {
            trace: trace.clone(),
            theme,
            dialogs: Mutex::new(request["dialogs"].as_array().cloned().unwrap_or_default()),
        })),
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
                std::fs::write(path, content).unwrap();
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
                        if let Ok(bytes) = std::fs::read(&path) {
                            if !bytes.is_empty() {
                                break bytes;
                            }
                        }
                        rx.recv().await.unwrap().unwrap();
                    }
                })
                .await
                .expect("command fixture timeout");
                let args: Value = serde_json::from_slice(&bytes).unwrap();
                trace.lock().unwrap().push(json!(["exec", args]));
            }
            "advance" => {
                tokio::time::advance(std::time::Duration::from_millis(step["ms"].as_u64().unwrap())).await;
                tokio::task::yield_now().await;
            }

            "bus" => {
                assert!(
                    bus.emit(step["channel"].as_str().unwrap(), step["data"].clone())
                        .is_empty()
                );
            }
            "prompt" => {
                *prompt.lock().unwrap() = step["value"].as_str().unwrap().into();
            }
            "before" => {
                let result = runner
                    .emit_before_agent_start("task", None, step["value"].as_str().unwrap(), Default::default())
                    .await;
                trace
                    .lock()
                    .unwrap()
                    .push(json!(["before", result.and_then(|result| result.system_prompt)]));
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
                entries.append_label_change(&id, step["value"].as_str()).unwrap();
            }
            "tool" => {
                let tool = &extension.tools.get(step["name"].as_str().unwrap()).unwrap().definition;
                let result = (tool.execute)(
                    "fixture-call".into(),
                    step["params"].clone(),
                    None,
                    None,
                    runner.create_context(),
                )
                .await
                .unwrap();
                trace.lock().unwrap().push(json!(["tool", result]));
            }
            other => panic!("unknown step {other}"),
        }
    }
    json!({"commands":commands, "tools":tools, "trace":trace.lock().unwrap().clone()})
}

pub fn factory(register: fn(&ExtensionApi) -> Result<(), HandlerError>) -> impl FnOnce(Trace) -> ExtensionFactory {
    move |_trace| {
        Arc::new(move |api| {
            let result = register(api);
            Box::pin(async move { result })
        })
    }
}
