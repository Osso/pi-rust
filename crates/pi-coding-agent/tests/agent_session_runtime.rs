//! Behavioral tests for `core/agent-session-runtime.ts`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::types::{AgentMessage, AgentTool};
use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{FauxModelDefinition, RegisterFauxProviderOptions, faux_provider};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{Context, Model};
use pi_coding_agent::core::agent_session::{AgentSession, AgentSessionConfig, ToolDefinition};
use pi_coding_agent::core::agent_session_runtime::{
    AgentSessionRuntime, CreateAgentSessionRuntimeOptions, CreateAgentSessionRuntimeResult, ForkPosition,
    NewSessionOptions, create_agent_session_runtime,
};
use pi_coding_agent::core::agent_session_services::{AgentSessionRuntimeDiagnostic, AgentSessionServices};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{
    ExtensionEvent, LoadExtensionsResult, SessionBeforeForkResult, SessionBeforeSwitchResult, SessionShutdownEvent,
};
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::session_manager::{SessionManager, lock_session_manager};
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
    pi_coding_agent::core::extensions::load_extension_from_factory(factory, cwd, event_bus, runtime.clone(), path)
        .await
        .expect("extension factory succeeds")
}

struct Harness {
    runtime: AgentSessionRuntime,
    logs: Arc<Mutex<Vec<String>>>,
    _temp: tempfile::TempDir,
}

async fn build_harness(cancel_switch: bool, cancel_fork: bool) -> Harness {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().display().to_string();
    let faux = Arc::new(faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        api: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".to_owned()),
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
    let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(credentials))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("runtime");
    model_runtime.register_native_provider(Arc::clone(&faux.provider));

    let extension_runtime = create_extension_runtime();
    let logs = Arc::new(Mutex::new(Vec::new()));
    let switch_logs = Arc::clone(&logs);
    let fork_logs = Arc::clone(&logs);
    let shutdown_logs = Arc::clone(&logs);
    let extension = load_extension(
        &extension_runtime,
        &cwd,
        &temp.path().join("runtime-extension.ts").display().to_string(),
        move |api| {
            api.on_session_before_switch(handler!(
                pi_coding_agent::core::extensions::SessionBeforeSwitchEvent,
                [switch_logs],
                |event, _ctx| {
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
                        .push(format!("before_fork:{}", event.entry_id));
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
    let session_manager = SessionManager::create(&cwd, Some(&session_dir), None).expect("session manager");
    let control_db = Arc::new(temp.path().join("control.db").display().to_string());

    let factory = {
        let loader = Arc::clone(&loader);
        let faux = Arc::clone(&faux);
        let model_runtime = model_runtime.clone();
        let control_db = Arc::clone(&control_db);
        Arc::new(move |request: CreateAgentSessionRuntimeOptions| {
            let loader = Arc::clone(&loader);
            let faux = Arc::clone(&faux);
            let model_runtime = model_runtime.clone();
            let control_db = Arc::clone(&control_db);
            Box::pin(async move {
                let model = faux.get_model(None).expect("faux model");
                let stream_runtime = model_runtime.clone();
                let agent = Agent::new(AgentOptions {
                    initial_state: Some(AgentInitialState {
                        model: Some(model),
                        system_prompt: Some(String::new()),
                        thinking_level: Some(ThinkingLevel::Off),
                        tools: Some(Vec::new()),
                        messages: None,
                    }),
                    stream_fn: Some(Arc::new(
                        move |model: Model, context: Context, options: SimpleStreamOptions| {
                            let runtime = stream_runtime.clone();
                            Box::pin(async move { runtime.stream_simple(model, context, options) })
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
                    model_runtime: model_runtime.clone(),
                    initial_active_tool_names: Some(Vec::new()),
                    allowed_tool_names: None,
                    excluded_tool_names: None,
                    permission_prompt_tool: None,
                    agent_dir: Some(request.agent_dir.clone()),
                    control_db_path: Some((*control_db).clone()),
                    base_tools: Vec::<Arc<dyn AgentTool>>::new(),
                    custom_tool_definitions: Vec::new(),
                    custom_tools: Vec::<ToolDefinition>::new(),
                    multi_agent_runtime_role: None,
                    multi_agent_execution_capability: None,
                    thinking_phase_timeout_ms: None,
                    supervisor_decision_requester: None,
                    multi_agent_store: None,
                    multi_agent_agent_id: None,
                    multi_agent_parent_session_id: None,
                    disable_runtime_coordination_inbound: false,
                    on_shared_channel_message_delivered: None,
                    extension_runner_ref: None,
                    session_start_event: None,
                })
                .map_err(|error| error.message().to_owned())?;
                Ok(CreateAgentSessionRuntimeResult {
                    session,
                    services: AgentSessionServices {
                        cwd: request.cwd,
                        agent_dir: request.agent_dir,
                        model_runtime,
                        settings_manager,
                        resource_loader: loader as Arc<dyn pi_coding_agent::core::resource_loader::ResourceLoader>,
                        diagnostics: Vec::<AgentSessionRuntimeDiagnostic>::new(),
                    },
                    diagnostics: Vec::new(),
                    model_fallback_message: None,
                })
            }) as pi_ai::request::BoxFuture<'static, Result<CreateAgentSessionRuntimeResult, String>>
        }) as pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeFactory
    };

    let runtime = create_agent_session_runtime(factory, &cwd, &cwd, session_manager)
        .await
        .expect("runtime");
    Harness {
        runtime,
        logs,
        _temp: temp,
    }
}

#[tokio::test]
async fn new_session_emits_before_switch_and_shutdown_and_runs_with_session() {
    let mut harness = build_harness(false, false).await;
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
                                custom_type: "note".to_owned(),
                                content: pi_coding_agent::core::extensions::types::CustomMessageContent::Text(
                                    "hello".to_owned(),
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
async fn before_switch_and_before_fork_cancellation_keep_the_current_session() {
    let mut harness = build_harness(true, true).await;
    let session_id = harness.runtime.session().session_id();

    let switched = harness
        .runtime
        .switch_session("/does/not/exist.jsonl", None, None)
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
        .fork("missing", ForkPosition::Before, None)
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
    let mut harness = build_harness(false, false).await;
    let error = harness
        .runtime
        .fork("missing", ForkPosition::Before, None)
        .await
        .expect_err("invalid entry");
    assert_eq!(error, "Invalid entry ID for forking");
}

#[tokio::test]
async fn relocate_updates_cwd_and_appends_a_context_note() {
    let mut harness = build_harness(false, false).await;
    let target = tempfile::tempdir().expect("target dir");
    let target_cwd = target.path().display().to_string();
    harness.runtime.relocate(&target_cwd).await.expect("relocate");
    assert_eq!(harness.runtime.session().cwd(), target_cwd);
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

#[tokio::test]
async fn import_from_jsonl_switches_to_the_imported_session() {
    let mut harness = build_harness(false, false).await;
    let source = tempfile::tempdir().expect("source dir");
    let source_cwd = source.path().display().to_string();
    let source_sessions = source.path().join("sessions").display().to_string();
    let mut source_manager = SessionManager::create(&source_cwd, Some(&source_sessions), None).expect("source manager");
    source_manager
        .append_message(AgentMessage::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text("imported".to_owned()),
            input_source: None,
            provider_native: None,
            timestamp: 1_700_000_000_000,
        }))
        .expect("append");
    source_manager.persist_for_recovery().expect("persist");
    let source_file = source_manager.get_session_file().expect("source file").to_owned();

    let result = harness
        .runtime
        .import_from_jsonl(&source_file, Some(&source_cwd))
        .await
        .expect("import");
    assert!(!result.cancelled);
    assert_eq!(harness.runtime.session().cwd(), source_cwd);
    let logs = harness.logs.lock().unwrap().clone();
    assert!(logs.contains(&"before_switch:resume".to_owned()), "logs: {logs:?}");
}

#[allow(dead_code)]
fn assert_extension_event_compiles(event: &ExtensionEvent) -> bool {
    matches!(event, ExtensionEvent::SessionShutdown(_))
}

#[tokio::test]
async fn session_replacement_rebinds_the_shared_extension_handle() {
    let mut harness = build_harness(false, false).await;
    let original_handle = harness.runtime.session().session_manager_handle();

    let source = tempfile::tempdir().expect("source dir");
    let source_cwd = source.path().display().to_string();
    let source_sessions = source.path().join("sessions").display().to_string();
    let mut source_manager = SessionManager::create(&source_cwd, Some(&source_sessions), None).expect("source manager");
    source_manager.persist_for_recovery().expect("persist source");
    let source_file = source_manager.get_session_file().expect("source file").to_owned();

    harness
        .runtime
        .switch_session(&source_file, Some(&source_cwd), None)
        .await
        .expect("switch");

    let session = harness.runtime.session();
    let live_handle = session.session_manager_handle();
    assert!(!Arc::ptr_eq(&original_handle, &live_handle));
    let context = session.extension_runner().create_context();
    let extension_handle = context.session_manager().expect("session manager");
    assert!(Arc::ptr_eq(&extension_handle, &live_handle));
    let extension_session_id = lock_session_manager(&extension_handle).get_session_id().to_owned();
    assert_eq!(extension_session_id, session.session_id());

    {
        let mut manager = lock_session_manager(&extension_handle);
        manager
            .append_session_info("renamed-by-extension")
            .expect("append session info");
    }
    assert_eq!(
        session.session_manager().get_session_name().as_deref(),
        Some("renamed-by-extension")
    );
}
