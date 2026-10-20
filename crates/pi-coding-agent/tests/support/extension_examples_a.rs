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
}
impl ExtensionUiContext for CaptureUi {
    fn notify(&self, message: &str, level: Option<NotifyType>) {
        self.trace.lock().unwrap().push(json!(["notify", message, level]));
    }
    fn set_status(&self, key: &str, text: Option<&str>) {
        self.trace.lock().unwrap().push(json!(["status", key, text]));
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
    let extension = load_extension_from_factory(
        registry.get("<example:batch-a>").unwrap().clone(),
        "/fixture",
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
        "/fixture",
        manager.clone(),
        Arc::new(ModelRegistry::new(model_runtime)),
        None,
    );
    let prompt = Arc::new(Mutex::new(String::new()));
    let mut context = support::context_actions();
    let current_prompt = prompt.clone();
    context.get_system_prompt = Arc::new(move || current_prompt.lock().unwrap().clone());
    let mut actions = support::actions();
    let session_name: Arc<Mutex<Option<String>>> = Arc::default();
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
    runner.bind_core(actions, context, None);
    let log = trace.clone();
    runner.bind_command_context(Some(ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
        set_sandbox_profile: Arc::new(|_, _| Ok(())),
        wait_for_idle: Arc::new(|| Box::pin(async {})),
        new_session: Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        fork: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        navigate_tree: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        switch_session: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        reload: Arc::new(move || {
            let log = log.clone();
            Box::pin(async move {
                log.lock().unwrap().push(json!(["reload"]));
            })
        }),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
    }));
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
        })),
        ExtensionMode::Tui,
    );
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
            "event" => {
                let mut event: ExtensionEvent = serde_json::from_value(step["event"].clone()).unwrap();
                runner.emit(&mut event).await;
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
                    if request["updates"].as_bool() == Some(true) {
                        let log = trace.clone();
                        Some(Arc::new(move |update| {
                            log.lock().unwrap().push(json!(["update", update]))
                        }))
                    } else {
                        None
                    },
                    runner.create_context(),
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
