//! Behavioral tests for `core/agent-session-runtime.ts`.

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::types::{AgentMessage, AgentTool};
use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{FauxModelDefinition, RegisterFauxProviderOptions, faux_provider};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{Context, Model};
use pi_coding_agent::core::agent_session::{
    AgentSession, AgentSessionConfig, PromptOptions, ToolDefinition, WorkingDirectoryRecoveryHandlers,
};
use pi_coding_agent::core::agent_session_runtime::{
    AgentSessionRuntime, CreateAgentSessionRuntimeOptions, CreateAgentSessionRuntimeResult, ForkPosition,
    NewSessionOptions, create_agent_session_runtime,
};
use pi_coding_agent::core::agent_session_services::{AgentSessionRuntimeDiagnostic, AgentSessionServices};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{
    ExtensionEvent, LoadExtensionsResult, SessionBeforeForkResult, SessionBeforeSwitchResult, SessionShutdownEvent,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::session_control_db::{
    WritableSessionMetadata, read_session_metadata, read_session_name_state, write_session_metadata,
};
use pi_coding_agent::core::session_manager::{SessionManager, SetMetadataControlDbPathOptions, lock_session_manager};
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use serde_json::json;

macro_rules! handler {
    ($ty:ty, [$($cap:ident),+], |$event:ident, $ctx:ident| $body:expr) => {{
        $(let $cap = Arc::clone(&$cap);)+
        move |$event: $ty, $ctx: pi_coding_agent::core::extensions::ExtensionContext| {
            $(let $cap = Arc::clone(&$cap);)+
            Box::pin(async move { $body })
                as pi_coding_agent::core::extensions::BoxFuture<
                    'static,
                    Result<Option<_>, pi_coding_agent::core::extensions::HandlerError>,
                >
        }
    }};
}

fn create_extension_runtime() -> pi_coding_agent::core::extensions::ExtensionRuntime {
    pi_coding_agent::core::extensions::create_extension_runtime()
}

async fn load_extension(
    runtime: &pi_coding_agent::core::extensions::ExtensionRuntime,
    cwd: &str,
    path: &str,
    setup: impl FnOnce(&pi_coding_agent::core::extensions::ExtensionApi) + Send + 'static,
) -> pi_coding_agent::core::extensions::Extension {
    use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
    let setup = Mutex::new(Some(setup));
    let factory: pi_coding_agent::core::extensions::ExtensionFactory = Arc::new(move |api| {
        let setup = setup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .expect("extension setup runs once");
        setup(api);
        Box::pin(async { Ok(()) })
    });
    let event_bus: Arc<dyn EventBus> = Arc::new(EventBusController::new());
    pi_coding_agent::core::extensions::load_extension_from_factory(
        factory,
        &JsString::from(cwd),
        event_bus,
        runtime.clone(),
        &JsString::from(path),
    )
    .await
    .expect("extension factory succeeds")
}

#[path = "../examples/extensions/dynamic_tools.rs"]
mod dynamic_tools_example;
#[path = "../examples/extensions/hello.rs"]
mod hello_example;

#[path = "support/runtime_callback_failures.rs"]
mod runtime_callback_failures;
#[path = "support/runtime_restart.rs"]
mod runtime_restart;

struct Harness {
    runtime: AgentSessionRuntime,
    logs: Arc<Mutex<Vec<String>>>,
    extension_context: Arc<Mutex<Option<pi_coding_agent::core::extensions::ExtensionContext>>>,
    faux: Arc<pi_ai::providers::faux::FauxProviderHandle>,
    _temp: tempfile::TempDir,
}

async fn build_harness(cancel_switch: bool, cancel_fork: bool) -> Harness {
    build_harness_with_dynamic_tools(cancel_switch, cancel_fork, false).await
}

async fn build_harness_with_dynamic_tools(cancel_switch: bool, cancel_fork: bool, dynamic_tools: bool) -> Harness {
    build_harness_with_runtime_observer(cancel_switch, cancel_fork, dynamic_tools, None).await
}

type RuntimeFactoryObserver = Arc<dyn Fn(&CreateAgentSessionRuntimeOptions) + Send + Sync>;

async fn build_harness_with_runtime_observer(
    cancel_switch: bool,
    cancel_fork: bool,
    dynamic_tools: bool,
    observer: Option<RuntimeFactoryObserver>,
) -> Harness {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().display().to_string();
    let faux = Arc::new(faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        api: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".into()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    }));
    let mut credentials = serde_json::Map::new();
    credentials.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(credentials)).unwrap();
    scripted_registry::register_scripted_provider(&model_registry, Arc::clone(&faux.provider));

    let extension_runtime = create_extension_runtime();
    let logs = Arc::new(Mutex::new(Vec::new()));
    let command_logs = Arc::clone(&logs);
    let switch_logs = Arc::clone(&logs);
    let fork_logs = Arc::clone(&logs);
    let shutdown_logs = Arc::clone(&logs);
    let extension_context: Arc<Mutex<Option<pi_coding_agent::core::extensions::ExtensionContext>>> =
        Arc::new(Mutex::new(None));
    let switch_context = Arc::clone(&extension_context);
    let extension = load_extension(
        &extension_runtime,
        &cwd,
        &temp.path().join("runtime-extension.ts").display().to_string(),
        move |api| {
            let cwd_logs = Arc::clone(&command_logs);
            api.register_command(
                "record-cwd",
                None,
                None,
                Arc::new(move |_, context| {
                    let cwd_logs = Arc::clone(&cwd_logs);
                    Box::pin(async move {
                        cwd_logs
                            .lock()
                            .unwrap()
                            .push(format!("command-cwd:{}", encode_node_utf8(&context.cwd()?)));
                        Ok(())
                    })
                }),
            )
            .unwrap();
            if dynamic_tools {
                dynamic_tools_example::dynamic_tools_extension(api).unwrap();
                let live = api.clone();
                api.register_command(
                    "register-hello",
                    None,
                    None,
                    Arc::new(move |_, _| {
                        let live = live.clone();
                        let command_logs = command_logs.clone();
                        Box::pin(async move {
                            hello_example::hello_extension(&live)?;
                            live.register_command(
                                "late-session-command",
                                None,
                                None,
                                Arc::new(move |_, _| {
                                    let command_logs = command_logs.clone();
                                    Box::pin(async move {
                                        command_logs.lock().unwrap().push("late command executed".into());
                                        Ok(())
                                    })
                                }),
                            )?;
                            Ok(())
                        })
                    }),
                )
                .unwrap();
            }
            api.on_session_before_switch(handler!(
                pi_coding_agent::core::extensions::SessionBeforeSwitchEvent,
                [switch_logs, switch_context],
                |event, ctx| {
                    *switch_context.lock().unwrap() = Some(ctx);
                    switch_logs
                        .lock()
                        .unwrap()
                        .push(format!("before_switch:{:?}", event.reason).to_lowercase());
                    Ok(cancel_switch.then_some(SessionBeforeSwitchResult { cancel: Some(true) }))
                }
            ))
            .unwrap();
            api.on_session_before_fork(handler!(
                pi_coding_agent::core::extensions::SessionBeforeForkEvent,
                [fork_logs],
                |event, _ctx| {
                    fork_logs
                        .lock()
                        .unwrap()
                        .push(encode_node_utf8(&js_format!("before_fork:{}", event.entry_id)));
                    Ok(cancel_fork.then_some(SessionBeforeForkResult {
                        cancel: Some(true),
                        skip_conversation_restore: None,
                    }))
                }
            ))
            .unwrap();
            api.on_session_shutdown({
                let shutdown_logs = Arc::clone(&shutdown_logs);
                move |event: SessionShutdownEvent, _ctx: pi_coding_agent::core::extensions::ExtensionContext| {
                    let shutdown_logs = Arc::clone(&shutdown_logs);
                    Box::pin(async move {
                        shutdown_logs
                            .lock()
                            .unwrap()
                            .push(format!("shutdown:{:?}", event.reason).to_lowercase());
                        Ok(None)
                    })
                        as pi_coding_agent::core::extensions::BoxFuture<
                            'static,
                            Result<Option<()>, pi_coding_agent::core::extensions::HandlerError>,
                        >
                }
            })
            .unwrap();
        },
    )
    .await;
    let loader = Arc::new(StaticResourceLoader {
        extensions: Some(LoadExtensionsResult {
            extensions: vec![extension],
            errors: Vec::new(),
            runtime: extension_runtime,
        }),
        ..StaticResourceLoader::default()
    });

    let session_dir = temp.path().join("sessions").display().to_string();
    let session_manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
        .expect("session manager");
    let control_db = Arc::new(temp.path().join("control.db").display().to_string());

    let factory = {
        let loader = Arc::clone(&loader);
        let faux = Arc::clone(&faux);
        let model_registry = model_registry.clone();
        let control_db = Arc::clone(&control_db);
        Arc::new(move |request: CreateAgentSessionRuntimeOptions| {
            if let Some(observer) = &observer {
                observer(&request);
            }
            let loader = Arc::clone(&loader);
            let faux = Arc::clone(&faux);
            let model_registry = model_registry.clone();
            let control_db = Arc::clone(&control_db);
            Box::pin(async move {
                let model = faux.get_model(None).expect("faux model");
                let stream_provider = Arc::clone(&faux.provider);
                let agent = Agent::new(AgentOptions {
                    initial_state: Some(AgentInitialState {
                        model: Some(model),
                        system_prompt: Some(JsString::default()),
                        thinking_level: Some(ThinkingLevel::Off),
                        tools: Some(Vec::new()),
                        messages: None,
                    }),
                    stream_fn: Some(Arc::new(
                        move |model: Model, context: Context, options: SimpleStreamOptions| {
                            let provider = Arc::clone(&stream_provider);
                            Box::pin(async move {
                                provider
                                    .stream_simple(model, context, options)
                                    .map_err(pi_agent_core::agent::AgentError::from)
                            })
                        },
                    )),
                    ..Default::default()
                })
                .expect("agent");
                let settings_manager = Arc::new(Mutex::new(SettingsManager::in_memory(
                    Default::default(),
                    SettingsManagerCreateOptions::default(),
                )));
                let session = AgentSession::new(AgentSessionConfig {
                    agent,
                    session_manager: Arc::new(Mutex::new(request.session_manager)),
                    settings_manager: Arc::clone(&settings_manager),
                    cwd: request.cwd.clone(),
                    scoped_models: Vec::new(),
                    resource_loader: Arc::clone(&loader)
                        as Arc<dyn pi_coding_agent::core::resource_loader::ResourceLoader>,
                    model_registry: model_registry.clone(),
                    initial_active_tool_names: Some(Vec::new()),
                    allowed_tool_names: None,
                    excluded_tool_names: None,
                    permission_prompt_tool: None,
                    agent_dir: Some(request.agent_dir.clone()),
                    control_db_path: Some(JsString::from((*control_db).clone())),
                    base_tools: Vec::<Arc<dyn AgentTool>>::new(),
                    custom_tool_definitions: Vec::new(),
                    custom_tools: Vec::<ToolDefinition>::new(),
                    multi_agent_runtime_role: None,
                    multi_agent_execution_capability: None,
                    thinking_phase_timeout_ms: None,
                    no_supervisor: None,
                    supervisor_decision_requester: None,
                    multi_agent_store: None,
                    multi_agent_agent_id: None,
                    multi_agent_parent_session_id: None,
                    multi_agent_requires_agent_id: false,
                    disable_runtime_coordination_inbound: false,
                    on_shared_channel_message_delivered: None,
                    extension_runner_ref: None,
                    session_start_event: None,
                })
                .map_err(|error| error.message().clone())?;
                Ok(CreateAgentSessionRuntimeResult {
                    session,
                    services: AgentSessionServices {
                        cwd: request.cwd,
                        agent_dir: request.agent_dir,
                        auth_storage: model_registry.auth_storage().clone(),
                        model_registry,
                        settings_manager,
                        resource_loader: loader as Arc<dyn pi_coding_agent::core::resource_loader::ResourceLoader>,
                        diagnostics: Vec::<AgentSessionRuntimeDiagnostic>::new(),
                    },
                    diagnostics: Vec::new(),
                    model_fallback_message: None,
                })
            })
                as pi_ai::request::BoxFuture<
                    'static,
                    Result<CreateAgentSessionRuntimeResult, pi_ai::request::CallbackError>,
                >
        }) as pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeFactory
    };

    let runtime = create_agent_session_runtime(
        factory,
        &JsString::from(&cwd),
        &JsString::from(&cwd),
        session_manager,
        None,
    )
    .await
    .expect("runtime");
    Harness {
        runtime,
        logs,
        extension_context,
        faux,
        _temp: temp,
    }
}

#[tokio::test]
async fn missing_import_preserves_the_frozen_error_name_and_file_path() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let harness = build_harness(false, false).await;
    let path = JsString::from(harness._temp.path().join("missing.jsonl").to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("runtime-import-error-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"path":path}));
    assert_eq!(expected["ok"], true, "{expected}");
    let error = harness.runtime.import_from_jsonl(&path, None).await.unwrap_err();
    let actual = error.thrown_value().as_error().unwrap().value();
    assert_eq!(actual, JsValue::from(&expected["output"]));
}

#[tokio::test]
async fn continue_recovers_deleted_session_cwd_before_empty_transcript_returns() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let harness = build_harness(false, false).await;
    let project = harness._temp.path().join("deleted-project");
    std::fs::create_dir(&project).unwrap();
    harness
        .runtime
        .relocate(&project.to_str().unwrap().into(), None)
        .await
        .unwrap();
    harness.runtime.session().agent().set_messages(Vec::new());
    let mut source = oracle::TsOracle::spawn(
        "agent-session-runtime-cwd-recovery-oracle.mts",
        "packages/coding-agent/src",
    );
    let expected = source.request(&json!({"cwd":project,"operation":"continue"}));
    assert_eq!(expected, json!({"ok":true,"cwd":harness._temp.path()}));
    harness.runtime.session().continue_run().await.unwrap();
    assert_eq!(json!({"ok":true,"cwd":harness.runtime.cwd()}), expected);
}

#[tokio::test]
async fn recovery_precedes_unknown_prompt_validation_and_forwards_real_prompts_and_commands() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let mut source = oracle::TsOracle::spawn(
        "agent-session-runtime-cwd-recovery-oracle.mts",
        "packages/coding-agent/src",
    );
    for (operation, text) in [
        ("unknown", "/not-a-command"),
        ("prompt", "recovered prompt"),
        ("command", "/record-cwd"),
    ] {
        let harness = build_harness_with_dynamic_tools(false, false, true).await;
        harness.runtime.set_rebind_session(Some(Arc::new(|session| {
            Box::pin(async move {
                session
                    .bind_extensions(pi_coding_agent::core::agent_session::ExtensionBindings::default())
                    .await
                    .map_err(pi_ai::request::CallbackError::from)
            })
        })));
        let project = harness._temp.path().join("deleted-project");
        std::fs::create_dir(&project).unwrap();
        harness
            .runtime
            .relocate(&project.to_str().unwrap().into(), None)
            .await
            .unwrap();
        let expected = source.request(&json!({"cwd":project,"operation":operation}));
        harness.faux.set_responses(vec![
            pi_ai::providers::faux::faux_assistant_message("completed", Default::default()).into(),
        ]);
        let result = harness.runtime.session().prompt(text, PromptOptions::default()).await;
        let mut actual = match result {
            Ok(()) => json!({"ok":true,"cwd":harness.runtime.cwd()}),
            Err(error) => json!({"ok":false,"cwd":harness.runtime.cwd(),"message":error.message()}),
        };
        if operation == "command" {
            let logs = harness.logs.lock().unwrap();
            let cwd = logs
                .iter()
                .find_map(|line| line.strip_prefix("command-cwd:"))
                .expect("registered callback ran");
            actual["commandCwd"] = json!(cwd);
        }
        assert_eq!(actual, expected, "operation {operation}");
        assert_eq!(harness.runtime.cwd(), harness._temp.path().to_str().unwrap());
        harness.runtime.dispose().await.unwrap();
    }
}

fn inactive_recovery_handlers() -> WorkingDirectoryRecoveryHandlers {
    WorkingDirectoryRecoveryHandlers {
        before_run: Arc::new(|| Box::pin(async { Ok(None) })),
        after_turn: Arc::new(|| Box::pin(async { Ok(()) })),
    }
}

#[tokio::test]
async fn before_run_rejection_keeps_raw_utf16_value_outside_command_error_catch() {
    use pi_js_compat::abort::AbortReason;
    for command in [false, true] {
        let harness = build_harness_with_dynamic_tools(false, false, true).await;
        let raw = pi_js_compat::js_json!({"reason":JsString::from_utf16(vec![0xd800,65,0xdc00]),"attempt":7});
        let reason = AbortReason::Value(raw);
        let thrown = reason.clone();
        let mut handlers = inactive_recovery_handlers();
        handlers.before_run = Arc::new(move || {
            let thrown = thrown.clone();
            Box::pin(async move { Err(pi_ai::request::CallbackError::from_rejection(thrown)) })
        });
        let session = harness.runtime.session();
        session.set_working_directory_recovery_handlers(handlers);
        let error = if command {
            session
                .prompt("/register-hello", PromptOptions::default())
                .await
                .unwrap_err()
        } else {
            session.continue_run().await.unwrap_err()
        };
        let error = pi_ai::request::CallbackError::from(error);
        assert_eq!(error.thrown_value(), reason);
        assert!(!session.get_all_tools().iter().any(|tool| tool.name == "hello"));
    }
}

#[test]
fn session_error_callback_source_retains_native_brand_and_properties() {
    use pi_js_compat::abort::AbortReason;
    let native = pi_js_compat::node_error::NativeNodeError::new("RangeError", JsString::from_utf16(vec![0xd800]))
        .with_property("code", JsValue::String("ERECOVER".into()))
        .with_property("details", pi_js_compat::js_json!({"attempt":7}));
    for reason in [AbortReason::Error(native.clone()), AbortReason::DomException(native)] {
        let wrapped = pi_coding_agent::core::agent_session::AgentSessionError::from(
            pi_ai::request::CallbackError::from_rejection(reason.clone()),
        );
        assert_eq!(pi_ai::request::CallbackError::from(wrapped).thrown_value(), reason);
    }
}

#[tokio::test]
async fn continue_recovery_can_replace_its_callbacks_and_reenter_the_start_lock() {
    let harness = build_harness(false, false).await;
    let session = harness.runtime.session();
    session.agent().set_messages(Vec::new());
    let inside = session.clone();
    let mut handlers = inactive_recovery_handlers();
    handlers.before_run = Arc::new(move || {
        let inside = inside.clone();
        Box::pin(async move {
            inside.set_working_directory_recovery_handlers(inactive_recovery_handlers());
            inside
                .continue_run()
                .await
                .map_err(pi_ai::request::CallbackError::from)?;
            Ok(Some(inside))
        })
    });
    session.set_working_directory_recovery_handlers(handlers);
    tokio::time::timeout(std::time::Duration::from_secs(5), session.continue_run())
        .await
        .unwrap()
        .unwrap();
    assert!(session.messages().is_empty());
}

#[tokio::test]
async fn after_turn_rejection_projects_only_at_the_frozen_agent_run_catch() {
    use pi_js_compat::abort::AbortReason;
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let mut source = oracle::TsOracle::spawn(
        "agent-session-runtime-cwd-recovery-oracle.mts",
        "packages/coding-agent/src",
    );
    let expected = source.request(&json!({"operation":"after-error","cwd":""}));
    let harness = build_harness(false, false).await;
    let native = pi_js_compat::node_error::NativeNodeError::new("RangeError", JsString::from_utf16(vec![0xd800]))
        .with_property("code", JsValue::String("ERECOVER".into()));
    let reason = AbortReason::Error(native);
    let thrown = reason.clone();
    let mut handlers = inactive_recovery_handlers();
    handlers.after_turn = Arc::new(move || {
        let thrown = thrown.clone();
        Box::pin(async move { Err(pi_ai::request::CallbackError::from_rejection(thrown)) })
    });
    let session = harness.runtime.session();
    session.set_working_directory_recovery_handlers(handlers);
    harness.faux.set_responses(vec![
        pi_ai::providers::faux::faux_assistant_message("completed", Default::default()).into(),
    ]);
    session.prompt("after turn", PromptOptions::default()).await.unwrap();
    let messages = session.messages();
    let failure = messages.last().unwrap().as_assistant().unwrap();
    assert_eq!(
        json!({"ok":true,"stopReason":failure.stop_reason,"message":failure.error_message.as_ref().unwrap().units()}),
        expected
    );
    assert_eq!(session.agent().error_message().unwrap().units(), &[0xd800]);
}

#[tokio::test]
async fn streaming_skips_before_run_and_keeps_registered_commands_available() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let harness = build_harness(false, false).await;
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let notify = entered.clone();
    let wait = release.clone();
    harness
        .faux
        .set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_, _, _, _| {
                let notify = notify.clone();
                let wait = wait.clone();
                Box::pin(async move {
                    notify.notify_one();
                    wait.notified().await;
                    Ok(pi_ai::providers::faux::faux_assistant_message(
                        "completed",
                        Default::default(),
                    ))
                })
            },
        ))]);
    let session = harness.runtime.session();
    let mut handlers = inactive_recovery_handlers();
    handlers.before_run = Arc::new(|| Box::pin(async { Err("must not run".into()) }));
    session.set_working_directory_recovery_handlers(handlers);
    let agent = session.agent().clone();
    let running = tokio::spawn(async move { agent.prompt("in flight").await });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    let error = session.continue_run().await.unwrap_err();
    session.prompt("/record-cwd", PromptOptions::default()).await.unwrap();
    let command_cwd = harness
        .logs
        .lock()
        .unwrap()
        .iter()
        .find_map(|line| line.strip_prefix("command-cwd:").map(str::to_owned))
        .unwrap();
    release.notify_one();
    running.await.unwrap().unwrap();
    let mut source = oracle::TsOracle::spawn(
        "agent-session-runtime-cwd-recovery-oracle.mts",
        "packages/coding-agent/src",
    );
    let expected = source.request(&json!({"operation":"streaming","cwd":harness.runtime.cwd()}));
    assert_eq!(json!({"message":error.message(),"commandCwd":command_cwd}), expected);
}

#[tokio::test]
async fn after_turn_prepares_recovery_before_stop_and_preserves_existing_preparation() {
    use pi_agent_core::agent::types::{AgentContext, ShouldStopAfterTurnContext};
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let mut source = oracle::TsOracle::spawn(
        "agent-session-after-turn-recovery-oracle.mts",
        "packages/coding-agent/src",
    );
    for prepared in [false, true] {
        let harness = build_harness(false, false).await;
        let project = harness._temp.path().join("deleted-project");
        let selected = harness._temp.path().join("selected");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&selected).unwrap();
        harness
            .runtime
            .relocate(&project.to_str().unwrap().into(), None)
            .await
            .unwrap();
        let session = harness.runtime.session();
        let runner = session.extension_runner();
        if prepared {
            runner
                .prepare_working_directory_relocation(&selected.to_str().unwrap().into())
                .await
                .unwrap();
        }
        let expected = source.request(&json!({"cwd":project,"selected":selected,"prepared":prepared}));
        assert_eq!(expected["ok"], true, "source {expected}");
        let hook = session.agent().should_stop_after_turn_hook().unwrap();
        let turn = ShouldStopAfterTurnContext {
            message: pi_ai::providers::faux::faux_assistant_message("turn ended", Default::default()),
            tool_results: Vec::new(),
            context: AgentContext {
                system_prompt: JsString::default(),
                messages: Vec::new(),
                tools: None,
            },
            new_messages: Vec::new(),
        };
        let stop = tokio::time::timeout(std::time::Duration::from_secs(5), hook(turn))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stop, expected["stop"]);
        assert_eq!(runner.has_prepared_tool_result_relocation(), expected["prepared"]);
        harness.runtime.activate_tool_result_relocation().await.unwrap();
        assert_eq!(json!(harness.runtime.cwd()), expected["target"]);
        harness.runtime.dispose().await.unwrap();
    }
}

#[tokio::test]
async fn real_turn_recovers_a_project_deleted_during_the_provider_response() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let harness = build_harness(false, false).await;
    let project = harness._temp.path().join("deleted-during-turn");
    std::fs::create_dir(&project).unwrap();
    harness
        .runtime
        .relocate(&project.to_str().unwrap().into(), None)
        .await
        .unwrap();
    let remove = project.clone();
    harness
        .faux
        .set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_, _, _, _| {
                let remove = remove.clone();
                Box::pin(async move {
                    std::fs::remove_dir(remove).unwrap();
                    Ok(pi_ai::providers::faux::faux_assistant_message(
                        "completed real turn",
                        Default::default(),
                    ))
                })
            },
        ))]);
    let original = harness.runtime.session();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        original.prompt("delete project during reply", PromptOptions::default()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!project.exists());
    assert!(
        original
            .messages()
            .iter()
            .filter_map(AgentMessage::as_assistant)
            .any(|message| {
                message.stop_reason == pi_ai::types::StopReason::Stop && message.content.iter().any(|content| {
            matches!(content, pi_ai::types::AssistantContent::Text(text) if text.text == "completed real turn")
        })
            })
    );
    std::fs::create_dir(&project).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "agent-session-after-turn-recovery-oracle.mts",
        "packages/coding-agent/src",
    );
    let expected = source.request(&json!({"cwd":project,"selected":harness._temp.path(),"prepared":false}));
    assert_eq!(expected["ok"], true);
    assert_eq!(json!(harness.runtime.cwd()), expected["target"]);
    assert!(
        !harness
            .runtime
            .session()
            .extension_runner()
            .has_prepared_tool_result_relocation()
    );
    harness.runtime.dispose().await.unwrap();
}

#[test]
fn deleted_process_cwd_is_repaired_with_the_exact_frozen_visible_notice() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let temp = tempfile::tempdir().unwrap();
    let source_deleted = temp.path().join("source-deleted");
    let native_deleted = temp.path().join("native-deleted");
    std::fs::create_dir(&source_deleted).unwrap();
    std::fs::create_dir(&native_deleted).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "agent-session-runtime-cwd-recovery-oracle.mts",
        "packages/coding-agent/src",
    );
    let expected = source.request(&json!({"operation":"repair","cwd":temp.path(),"deleted":source_deleted}));
    assert_eq!(expected["ok"], true, "source {expected}");
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "runtime_process_cwd_repair_child",
            "--nocapture",
        ])
        .env("PI_RUNTIME_REPAIR_ROOT", temp.path())
        .env("PI_RUNTIME_REPAIR_DELETED", native_deleted)
        .output()
        .unwrap();
    assert!(child.status.success(), "{}", String::from_utf8(child.stderr).unwrap());
    let stdout = String::from_utf8(child.stdout).unwrap();
    let actual: serde_json::Value = serde_json::from_str(
        stdout
            .lines()
            .find_map(|line| line.strip_prefix("RUNTIME_CWD_RECOVERY:"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
}

#[test]
#[ignore = "isolated process cwd mutation child invoked by parent test"]
fn runtime_process_cwd_repair_child() {
    use std::io::Write;
    let root = std::env::var("PI_RUNTIME_REPAIR_ROOT").unwrap();
    let deleted = std::env::var("PI_RUNTIME_REPAIR_DELETED").unwrap();
    let original = pi_js_compat::process_cwd::cwd().unwrap();
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let output = executor.block_on(async {
        let harness = build_harness(false, false).await;
        harness.runtime.relocate(&root.clone().into(), None).await.unwrap();
        harness.faux.set_responses(vec![
            pi_ai::providers::faux::faux_assistant_message("completed", Default::default()).into(),
        ]);
        pi_js_compat::process_cwd::chdir(&deleted.clone().into()).unwrap();
        std::fs::remove_dir(&deleted).unwrap();
        harness.runtime.session().continue_run().await.unwrap();
        let entries = harness.runtime.session().session_manager().get_entries();
        let notice = entries
            .iter()
            .map(|entry| serde_json::to_value(entry).unwrap())
            .find(|entry| entry["customType"] == "process_cwd_recovered")
            .expect("visible recovery notice");
        let output = json!({"ok":true,"cwd":pi_js_compat::process_cwd::cwd().unwrap(),"notice":{
            "customType":notice["customType"],"content":notice["content"],"display":notice["display"]
        }});
        harness.runtime.dispose().await.unwrap();
        output
    });
    pi_js_compat::process_cwd::chdir(&original).unwrap();
    writeln!(std::io::stdout(), "RUNTIME_CWD_RECOVERY:{output}").unwrap();
}

#[tokio::test]
async fn teardown_removes_a_persisted_empty_session_before_disposal() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let harness = build_harness(false, false).await;
    let source_root = harness._temp.path().join("source-fixture");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("empty-session-cleanup-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(
        &json!({"file":source_root.join("session.jsonl"),"db":source_root.join("control.db"),"cwd":source_root}),
    );
    assert_eq!(
        expected,
        json!({"ok":true,"fileExists":false,"metadataExists":false,"disposed":true})
    );
    let session = harness.runtime.session();
    let (file, db) = {
        let mut manager = session.session_manager();
        manager.persist_for_recovery().unwrap();
        (
            manager.get_session_file().unwrap().clone(),
            manager.get_metadata_control_db_path().unwrap().clone(),
        )
    };
    assert!(std::path::Path::new(&encode_node_utf8(&file)).exists());
    assert!(read_session_metadata(&db, &file).unwrap().is_some());
    let context = session.extension_runner().create_context();
    harness.runtime.dispose().await.unwrap();
    let actual = json!({"ok":true,"fileExists":std::path::Path::new(&encode_node_utf8(&file)).exists(),"metadataExists":read_session_metadata(&db,&file).unwrap().is_some(),"disposed":context.cwd().is_err()});
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn cleanup_unlink_failure_keeps_session_live_after_metadata_has_been_removed() {
    use pi_js_compat::abort::AbortReason;
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let harness = build_harness(false, false).await;
    let session = harness.runtime.session();
    let (file, db) = {
        let mut manager = session.session_manager();
        manager.persist_for_recovery().unwrap();
        (
            manager.get_session_file().unwrap().clone(),
            manager.get_metadata_control_db_path().unwrap().clone(),
        )
    };
    let mut source = oracle::TsOracle::spawn("empty-session-cleanup-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"file":file,"db":harness._temp.path().join("source-error.db"),"cwd":harness._temp.path(),"directory":true}));
    assert_eq!(expected["ok"], false, "source {expected}");
    assert!(read_session_metadata(&db, &file).unwrap().is_some());
    let context = session.extension_runner().create_context();
    let error = harness.runtime.dispose().await.unwrap_err();
    let native = match error.thrown_value() {
        AbortReason::Error(error) => error,
        reason => panic!("native unlink Error: {reason:?}"),
    };
    let value = native.value();
    let actual = json!({"ok":false,"fileExists":std::path::Path::new(&encode_node_utf8(&file)).exists(),"metadataExists":read_session_metadata(&db,&file).unwrap().is_some(),"disposed":context.cwd().is_err(),"error":{
        "name":native.name,"message":native.message.units(),"code":value.get("code"),"errno":value.get("errno"),"syscall":value.get("syscall"),"path":value.get("path").unwrap().as_string().unwrap().units()
    }});
    assert_eq!(actual, expected);
    assert!(context.cwd().is_ok());
    std::fs::remove_dir(encode_node_utf8(&file)).unwrap();
    harness.runtime.dispose().await.unwrap();
}

#[tokio::test]
async fn transition_teardown_does_not_abort_or_emit_extra_queue_updates_before_invalidation() {
    use pi_coding_agent::core::agent_session::AgentSessionEvent;
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let harness = build_harness(false, false).await;
    let source_root = harness._temp.path().join("source-queue-probe");
    std::fs::create_dir(&source_root).unwrap();
    let mut source = oracle::TsOracle::spawn("runtime-teardown-queue-events-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"root":source_root}));
    assert_eq!(
        expected,
        json!({"ok":true,"error":"fixture stop","events":[],"sameSession":true})
    );
    let session = harness.runtime.session();
    let id = session.session_id();
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured = events.clone();
    session.subscribe(Arc::new(move |event| {
        if matches!(event, AgentSessionEvent::QueueUpdate(_)) {
            captured.lock().unwrap().push("queue_update".into());
        }
    }));
    harness
        .runtime
        .set_before_session_invalidate(Some(Arc::new(|| Err("fixture stop".into()))));
    let error = harness
        .runtime
        .new_session(NewSessionOptions::default())
        .await
        .unwrap_err();
    let actual = json!({"ok":true,"error":error.message,"events":events.lock().unwrap().clone(),"sameSession":harness.runtime.session().session_id()==id});
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn prepared_relocation_holds_lifecycle_gate_until_activation() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let mut source = oracle::TsOracle::spawn("agent-session-runtime-gate-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({}));
    assert_eq!(expected["ok"], true, "{expected}");
    let harness = build_harness(false, false).await;
    let target = harness._temp.path().join("relocated");
    std::fs::create_dir(&target).unwrap();
    let shared = harness.runtime;
    shared
        .prepare_tool_result_relocation(&JsString::from(target.to_str().unwrap()))
        .await
        .unwrap();
    let queued_runtime = shared.clone();
    let queued = tokio::spawn(async move { queued_runtime.clone().new_session(NewSessionOptions::default()).await });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        queued.is_finished(),
        expected["output"]["beforeActivation"].as_bool().unwrap()
    );
    let _ = shared.activate_tool_result_relocation().await;
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), queued)
        .await
        .unwrap()
        .unwrap();
    assert!(!result.unwrap().cancelled);
    assert_eq!(expected["output"]["afterActivation"], true);
}

#[tokio::test]
async fn new_session_emits_before_switch_and_shutdown_and_runs_with_session() {
    let harness = build_harness(false, false).await;
    let called = Arc::new(AtomicBool::new(false));
    let callback_called = Arc::clone(&called);
    let result = harness
        .runtime
        .new_session(NewSessionOptions {
            parent_session: None,
            setup: None,
            with_session: Some(Arc::new(move |context| {
                let callback_called = Arc::clone(&callback_called);
                Box::pin(async move {
                    callback_called.store(true, Ordering::SeqCst);
                    assert!(
                        (context.send_message)(
                            pi_coding_agent::core::extensions::types::CustomMessagePayload {
                                custom_type: JsString::from("note"),
                                content: pi_coding_agent::core::extensions::types::CustomMessageContent::Text(
                                    JsString::from("hello"),
                                ),
                                display: true,
                                details: None,
                            },
                            Some(pi_coding_agent::core::extensions::types::SendMessageOptions {
                                trigger_turn: None,
                                deliver_as: Some(pi_coding_agent::core::extensions::types::MessageDelivery::NextTurn,),
                            }),
                        )
                        .await
                        .is_ok()
                    );
                    Ok(())
                })
            })),
        })
        .await
        .expect("new session");
    assert!(!result.cancelled);
    assert!(called.load(Ordering::SeqCst));
    let logs = harness.logs.lock().unwrap().clone();
    assert!(logs.contains(&"before_switch:new".to_owned()), "logs: {logs:?}");
    assert!(logs.contains(&"shutdown:new".to_owned()), "logs: {logs:?}");
}

#[tokio::test]
async fn new_session_invalidates_the_outgoing_extension_context() {
    let harness = build_harness(false, false).await;
    harness
        .runtime
        .new_session(NewSessionOptions::default())
        .await
        .expect("new session");
    let context = harness
        .extension_context
        .lock()
        .unwrap()
        .clone()
        .expect("session_before_switch captured the outgoing context");
    let error = context.cwd().expect_err("outgoing context is stale");
    assert_eq!(
        error.message,
        "This extension ctx is stale after session replacement or reload. Do not use a captured pi or command ctx after ctx.newSession(), ctx.fork(), ctx.switchSession(), or ctx.reload(). For newSession, fork, and switchSession, move post-replacement work into withSession and use the ctx passed to withSession. For reload, do not use the old ctx after await ctx.reload()."
    );
}

#[tokio::test]
async fn new_session_cleans_up_the_outgoing_session_resources() {
    let harness = build_harness(false, false).await;
    let outgoing_session_id = harness.runtime.session().session_id();
    let cleaned: Arc<Mutex<Vec<Option<JsString>>>> = Arc::new(Mutex::new(Vec::new()));
    let cleanup_log = Arc::clone(&cleaned);
    let registration = pi_ai::session_resources::register_session_resource_cleanup(Box::new(move |session_id| {
        cleanup_log.lock().unwrap().push(session_id.cloned());
        Ok(())
    }));

    harness
        .runtime
        .new_session(NewSessionOptions::default())
        .await
        .expect("new session");
    registration.unregister();

    let cleaned = cleaned.lock().unwrap().clone();
    assert!(
        cleaned.contains(&Some(outgoing_session_id.clone())),
        "outgoing {} not cleaned: {cleaned:?}",
        encode_node_utf8(&outgoing_session_id)
    );
}

#[tokio::test]
async fn before_switch_and_before_fork_cancellation_keep_the_current_session() {
    let harness = build_harness(true, true).await;
    let session_id = harness.runtime.session().session_id();

    let switched = harness
        .runtime
        .switch_session(&JsString::from("/does/not/exist.jsonl"), Default::default())
        .await
        .expect("switch");
    assert!(switched.cancelled);
    let created = harness
        .runtime
        .new_session(NewSessionOptions {
            parent_session: None,
            setup: None,
            with_session: None,
        })
        .await
        .expect("new");
    assert!(created.cancelled);
    let forked = harness
        .runtime
        .fork(&JsString::from("missing"), ForkPosition::Before, None)
        .await
        .expect("fork");
    assert!(forked.cancelled);
    assert_eq!(harness.runtime.session().session_id(), session_id);
    let logs = harness.logs.lock().unwrap().clone();
    assert!(logs.contains(&"before_switch:resume".to_owned()), "logs: {logs:?}");
    assert!(logs.contains(&"before_fork:missing".to_owned()), "logs: {logs:?}");
}

#[tokio::test]
async fn fork_rejects_an_unknown_entry() {
    let harness = build_harness(false, false).await;
    let error = harness
        .runtime
        .fork(&JsString::from("missing"), ForkPosition::Before, None)
        .await
        .expect_err("invalid entry");
    assert_eq!(error.message, "Invalid entry ID for forking");
}

#[tokio::test]
async fn relocate_updates_cwd_and_appends_a_context_note() {
    let harness = build_harness(false, false).await;
    let target = tempfile::tempdir().expect("target dir");
    let target_cwd = target.path().display().to_string();
    harness
        .runtime
        .relocate(&JsString::from(&target_cwd), None)
        .await
        .expect("relocate");
    assert_eq!(*harness.runtime.session().cwd(), target_cwd);
    let entries = harness.runtime.session().session_manager().get_entries();
    let has_note = entries.iter().any(|entry| {
        matches!(
            entry,
            pi_coding_agent::core::session_manager::SessionEntry::Known(
                pi_coding_agent::core::session_manager::KnownSessionEntry::CustomMessage(custom)
            ) if custom.custom_type == "cwd_changed"
        )
    });
    assert!(has_note, "expected a cwd_changed entry");
}

fn session_metadata_row(session_path: &str, id: &str, cwd: &str, name: Option<&str>) -> WritableSessionMetadata {
    WritableSessionMetadata {
        session_path: JsString::from(session_path.to_owned()),
        id: JsString::from(id.to_owned()),
        cwd: JsString::from(cwd.to_owned()),
        name: name.map(str::to_owned).map(JsString::from),
        parent_session_path: None,
        archived_at: None,
        is_archived: None,
        is_subagent: None,
        subagent_name: None,
        model_provider: None,
        model_id: None,
        thinking_level: None,
        created_at: JsString::from("2026-08-23T00:00:00.000Z"),
        modified_at: JsString::from("2026-08-23T00:00:00.000Z"),
        message_count: 0,
        first_message: JsString::from("(no messages)"),
    }
}

fn attach_control_db(harness: &Harness) -> String {
    let control_db_path = harness._temp.path().join("control.db").display().to_string();
    lock_session_manager(&harness.runtime.session().session_manager_handle())
        .set_metadata_control_db_path(
            Some(&JsString::from(&control_db_path)),
            SetMetadataControlDbPathOptions::default(),
        )
        .expect("attach control db");
    control_db_path
}

#[tokio::test]
async fn import_does_not_inherit_stale_metadata_or_historical_jsonl_names() {
    let harness = build_harness(false, false).await;
    let control_db_path = attach_control_db(&harness);
    let temp_dir = harness._temp.path().to_path_buf();
    let source_dir = temp_dir.join("import-source");
    std::fs::create_dir_all(&source_dir).expect("source dir");
    let source_path = source_dir.join("imported.jsonl");
    let session_dir = lock_session_manager(&harness.runtime.session().session_manager_handle())
        .get_session_dir()
        .to_owned();
    let destination_path = std::path::Path::new(&encode_node_utf8(&session_dir))
        .join("imported.jsonl")
        .display()
        .to_string();
    std::fs::write(
        &source_path,
        [
            json!({
                "type": "session",
                "version": 3,
                "id": "imported-session",
                "timestamp": "2026-08-23T00:00:00.000Z",
                "cwd": temp_dir.display().to_string(),
            })
            .to_string(),
            json!({
                "type": "session_info",
                "id": "historical-name",
                "parentId": null,
                "timestamp": "2026-08-23T00:00:01.000Z",
                "name": "Historical Import Name",
            })
            .to_string(),
            String::new(),
        ]
        .join("\n"),
    )
    .expect("source file");
    write_session_metadata(
        &control_db_path,
        &session_metadata_row(
            &destination_path,
            "stale-session",
            &temp_dir.display().to_string(),
            Some("Stale Destination Name"),
        ),
    )
    .expect("stale metadata");

    harness
        .runtime
        .import_from_jsonl(&JsString::from(&source_path.display().to_string()), None)
        .await
        .expect("import");

    let session = harness.runtime.session();
    assert_eq!(
        session.session_file().as_ref(),
        Some(destination_path.as_str()).map(JsString::from).as_ref()
    );
    assert_eq!(session.session_name(), None);
    assert!(
        !lock_session_manager(&session.session_manager_handle()).has_session_name_state(),
        "a copied import starts with no stored name state"
    );
    assert_eq!(
        read_session_metadata(&control_db_path, &destination_path)
            .expect("metadata")
            .and_then(|metadata| metadata.name),
        None
    );
    assert!(
        !read_session_name_state(&control_db_path, &destination_path)
            .expect("state")
            .has_stored_value
    );
}

#[tokio::test]
async fn fork_does_not_inherit_the_parent_session_name() {
    let harness = build_harness(false, false).await;
    let control_db_path = attach_control_db(&harness);
    let session = harness.runtime.session().clone();
    session.prompt("hello", PromptOptions::default()).await.expect("prompt");
    session.prompt("again", PromptOptions::default()).await.expect("prompt");
    session
        .set_session_name(&JsString::from("Parent Session Name"))
        .expect("set name");
    let previous_session_file = session.session_file();
    let leaf_id = lock_session_manager(&session.session_manager_handle())
        .get_leaf_id()
        .expect("leaf")
        .to_owned();

    let result = harness
        .runtime
        .fork(&leaf_id, ForkPosition::At, None)
        .await
        .expect("fork");
    assert!(!result.cancelled);
    let forked_session_file = harness.runtime.session().session_file().expect("forked file");
    assert_ne!(Some(&forked_session_file), previous_session_file.as_ref());
    let forked = harness.runtime.session();
    assert_eq!(forked.session_name(), None);
    assert!(!lock_session_manager(&forked.session_manager_handle()).has_session_name_state());
    assert_eq!(
        read_session_metadata(&control_db_path, &forked_session_file)
            .expect("metadata")
            .and_then(|metadata| metadata.name),
        None
    );
}

#[tokio::test]
async fn import_from_jsonl_switches_to_the_imported_session() {
    let harness = build_harness(false, false).await;
    let source = tempfile::tempdir().expect("source dir");
    let source_cwd = source.path().display().to_string();
    let source_sessions = source.path().join("sessions").display().to_string();
    let mut source_manager = SessionManager::create(
        &JsString::from(&source_cwd),
        Some(&JsString::from(&source_sessions)),
        None,
    )
    .expect("source manager");
    source_manager
        .append_message(AgentMessage::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text(JsString::from("imported")),
            input_source: None,
            provider_native: None,
            timestamp: 1_700_000_000_000.0,
        }))
        .expect("append");
    source_manager.persist_for_recovery().expect("persist");
    let source_file = source_manager.get_session_file().expect("source file").to_owned();

    let result = harness
        .runtime
        .import_from_jsonl(&source_file, Some(&JsString::from(&source_cwd)))
        .await
        .expect("import");
    assert!(!result.cancelled);
    assert_eq!(*harness.runtime.session().cwd(), source_cwd);
    let logs = harness.logs.lock().unwrap().clone();
    assert!(logs.contains(&"before_switch:resume".to_owned()), "logs: {logs:?}");
}

#[allow(dead_code)]
fn assert_extension_event_compiles(event: &ExtensionEvent) -> bool {
    matches!(event, ExtensionEvent::SessionShutdown(_))
}

#[tokio::test]
async fn session_replacement_rebinds_the_shared_extension_handle() {
    let harness = build_harness(false, false).await;
    let original_handle = harness.runtime.session().session_manager_handle();

    let source = tempfile::tempdir().expect("source dir");
    let source_cwd = source.path().display().to_string();
    let source_sessions = source.path().join("sessions").display().to_string();
    let mut source_manager = SessionManager::create(
        &JsString::from(&source_cwd),
        Some(&JsString::from(&source_sessions)),
        None,
    )
    .expect("source manager");
    source_manager.persist_for_recovery().expect("persist source");
    let source_file = source_manager.get_session_file().expect("source file").to_owned();

    harness
        .runtime
        .switch_session(
            &source_file,
            pi_coding_agent::core::agent_session_runtime::SwitchSessionOptions {
                cwd_override: Some(JsString::from(&source_cwd)),
                ..Default::default()
            },
        )
        .await
        .expect("switch");

    let session = harness.runtime.session();
    let live_handle = session.session_manager_handle();
    assert!(!Arc::ptr_eq(&original_handle, &live_handle));
    let context = session.extension_runner().create_context();
    let extension_handle = context.session_manager().expect("session manager");
    assert_eq!(
        extension_handle.get_session_file(),
        lock_session_manager(&live_handle).get_session_file().cloned()
    );
    let extension_session_id = extension_handle.get_session_id();
    assert_eq!(extension_session_id, session.session_id());

    session
        .set_session_name(&JsString::from("renamed-by-extension"))
        .expect("set session name through the API action");
    assert_eq!(extension_handle.get_session_name(), Some("renamed-by-extension".into()));
    assert_eq!(
        session.session_manager().get_session_name().as_ref(),
        Some(&JsString::from("renamed-by-extension"))
    );
}

#[tokio::test]
async fn bound_runtime_actions_relocate_through_the_extension_context() {
    let Harness {
        runtime,
        logs: _,
        extension_context: _,
        faux: _,
        _temp,
    } = build_harness(false, false).await;

    let session = runtime.clone().session().clone();
    session
        .session_manager()
        .append_message(AgentMessage::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text(JsString::from("relocate me")),
            input_source: None,
            provider_native: None,
            timestamp: 1.0,
        }))
        .expect("append user message");

    let first = _temp.path().join("first");
    std::fs::create_dir(&first).expect("first dir");
    let first = first.display().to_string();
    let session = runtime.clone().session().clone();
    let context = session.extension_runner().create_context();
    context
        .relocate(&JsString::from(&first))
        .expect("relocate handler is bound")
        .await
        .expect("relocate");
    assert_eq!(runtime.cwd(), first);

    let second = _temp.path().join("second");
    std::fs::create_dir(&second).expect("second dir");
    let second = second.display().to_string();
    let session = runtime.clone().session().clone();
    let context = session.extension_runner().create_context();
    context
        .relocate(&JsString::from(&second))
        .expect("relocate handler is rebound after replacement")
        .await
        .expect("relocate after replacement");
    assert_eq!(runtime.cwd(), second);

    let third = _temp.path().join("third");
    std::fs::create_dir(&third).expect("third dir");
    let third = third.display().to_string();
    let session = runtime.clone().session().clone();
    let runner = session.extension_runner();
    runner
        .create_context()
        .relocate_after_tool_result("call-1", &JsString::from(&third))
        .expect("schedule tool-result relocation");
    runner
        .deliver_tool_result_relocation("call-1")
        .await
        .expect("prepare tool-result relocation");
    assert!(runner.has_prepared_tool_result_relocation());
    runner
        .activate_tool_result_relocation()
        .await
        .expect("activate tool-result relocation");
    assert_eq!(runtime.cwd(), third);
}

#[tokio::test]
async fn before_invalidate_error_skips_dispose_and_releases_gate_for_next_transition() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    let mut source = oracle::TsOracle::spawn(
        "agent-session-runtime-invalidate-error-oracle.mts",
        "packages/coding-agent/src",
    );
    let raw = JsString::from_utf16(vec![104, 111, 111, 107, 32, 0xd800]);
    let expected = source.request(&json!({"error":raw.units()}));
    assert_eq!(expected["ok"], true, "source: {expected}");
    let harness = build_harness(false, false).await;
    let outgoing = harness.runtime.session();
    let id = outgoing.session_manager().get_session_id().clone();
    let context = outgoing.extension_runner().create_context();
    let fail = AtomicBool::new(true);
    harness.runtime.set_before_session_invalidate(Some(Arc::new(move || {
        if fail.swap(false, Ordering::SeqCst) {
            Err(pi_ai::request::CallbackError::new(raw.clone()))
        } else {
            Ok(())
        }
    })));
    let error = harness
        .runtime
        .new_session(NewSessionOptions::default())
        .await
        .err()
        .expect("hook rejection");
    let after_error = json!({"sameSession":harness.runtime.session().session_manager().get_session_id()==&id,"disposed":context.cwd().is_err()});
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        harness.runtime.new_session(NewSessionOptions::default()),
    )
    .await
    .expect("permit released")
    .unwrap();
    assert_eq!(
        json!({"error":error.message.units(),"afterError":after_error,"afterNext":{"changed":harness.runtime.session().session_manager().get_session_id()!=&id,"disposed":context.cwd().is_err()}}),
        expected["output"]
    );
}

#[tokio::test]
async fn dispose_emits_session_shutdown_quit_before_invalidating_the_session() {
    let harness = build_harness(false, false).await;
    let invalidated = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&invalidated);
    harness.runtime.set_before_session_invalidate(Some(Arc::new(move || {
        flag.store(true, Ordering::SeqCst);
        Ok(())
    })));

    harness.runtime.dispose().await.expect("runtime disposal succeeds");

    let logs = harness.logs.lock().unwrap().clone();
    assert!(logs.contains(&"shutdown:quit".to_owned()), "logs: {logs:?}");
    assert!(invalidated.load(Ordering::SeqCst));
}

#[cfg(unix)]
#[tokio::test]
async fn import_through_a_symlinked_session_dir_preserves_the_source_file() {
    let harness = build_harness(false, false).await;
    let source = tempfile::tempdir().expect("source dir");
    let source_cwd = source.path().display().to_string();
    let source_sessions = source.path().join("sessions");
    std::fs::create_dir_all(&source_sessions).expect("source sessions");
    let mut source_manager = SessionManager::create(
        &JsString::from(&source_cwd),
        Some(&JsString::from(&source_sessions.display().to_string())),
        None,
    )
    .expect("source manager");
    source_manager
        .append_message(AgentMessage::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text(JsString::from("symlinked import")),
            input_source: None,
            provider_native: None,
            timestamp: 1_700_000_000_001.0,
        }))
        .expect("append");
    source_manager.persist_for_recovery().expect("persist");
    let source_file = source_manager.get_session_file().expect("source file").to_owned();

    let session_dir = harness.runtime.session().session_manager().get_session_dir().to_owned();
    std::fs::remove_dir_all(&encode_node_utf8(&session_dir)).expect("remove harness session dir");
    std::os::unix::fs::symlink(&source_sessions, &encode_node_utf8(&session_dir)).expect("symlink session dir");

    let result = harness
        .runtime
        .import_from_jsonl(&source_file, Some(&JsString::from(&source_cwd)))
        .await
        .expect("import");
    assert!(!result.cancelled);
    let content = std::fs::read_to_string(&encode_node_utf8(&source_file)).expect("source file readable");
    assert!(content.contains("symlinked import"), "content: {content}");
}

#[tokio::test]
async fn late_tools_refresh_the_real_session_without_reactivating_disabled_tools() {
    use pi_coding_agent::core::agent_session::ExtensionBindings;
    use pi_coding_agent::core::extensions::{SessionStartEvent, SessionStartReason};
    let harness = build_harness_with_dynamic_tools(false, false, true).await;
    let session = harness.runtime.session();
    session
        .bind_extensions(ExtensionBindings::default())
        .await
        .expect("bind extensions");
    let runner = session.extension_runner();
    runner
        .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
            reason: SessionStartReason::Startup,
            previous_session_file: None,
        }))
        .await;
    assert!(session.get_all_tools().iter().any(|tool| tool.name == "echo_session"));
    assert!(session.get_active_tool_names().contains(&"echo_session".into()));
    let add = runner.get_prompt_command("add-echo-tool").unwrap();
    (add.handler)("NEW_TOOL".into(), runner.create_command_context())
        .await
        .unwrap();
    assert!(session.get_all_tools().iter().any(|tool| tool.name == "new_tool"));
    let tool = session
        .agent()
        .tools()
        .into_iter()
        .find(|tool| tool.name() == "new_tool")
        .unwrap();
    let result = tool
        .execute(
            "call-late".into(),
            JsValue::from(json!({"message":"from session"})),
            None,
            Arc::new(|_| Ok(())),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({"content":[{"type":"text","text":"[new_tool] from session"}],"details":{"tool":"new_tool","prefix":"[new_tool] "}})
    );
    let register = runner.get_prompt_command("register-hello").unwrap();
    (register.handler)(JsString::default(), runner.create_command_context())
        .await
        .unwrap();
    assert!(session.get_active_tool_names().contains(&"hello".into()));
    session
        .prompt("/late-session-command", PromptOptions::default())
        .await
        .unwrap();
    assert!(harness.logs.lock().unwrap().contains(&"late command executed".into()));
    session.set_active_tools_by_name(&[]).unwrap();
    (register.handler)(JsString::default(), runner.create_command_context())
        .await
        .unwrap();
    assert!(session.get_all_tools().iter().any(|tool| tool.name == "hello"));
    assert!(!session.get_active_tool_names().contains(&"hello".into()));
    (add.handler)("another_new".into(), runner.create_command_context())
        .await
        .unwrap();
    assert!(session.get_active_tool_names().contains(&"another_new".into()));
    assert!(!session.get_active_tool_names().contains(&"new_tool".into()));
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
