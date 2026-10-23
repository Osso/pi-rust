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
use pi_coding_agent::core::agent_session::{AgentSession, AgentSessionConfig, PromptOptions, ToolDefinition};
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

struct Harness {
    runtime: AgentSessionRuntime,
    logs: Arc<Mutex<Vec<String>>>,
    extension_context: Arc<Mutex<Option<pi_coding_agent::core::extensions::ExtensionContext>>>,
    _temp: tempfile::TempDir,
}

async fn build_harness(cancel_switch: bool, cancel_fork: bool) -> Harness {
    build_harness_with_dynamic_tools(cancel_switch, cancel_fork, false).await
}

async fn build_harness_with_dynamic_tools(cancel_switch: bool, cancel_fork: bool, dynamic_tools: bool) -> Harness {
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
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(credentials));
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
            let loader = Arc::clone(&loader);
            let faux = Arc::clone(&faux);
            let model_registry = model_registry.clone();
            let control_db = Arc::clone(&control_db);
            Box::pin(async move {
                let model = faux.get_model(None).expect("faux model");
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
                            Box::pin(async move {
                                pi_ai::compat::stream_simple(model, context, options)
                                    .map_err(pi_agent_core::AgentError::from)
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
            }) as pi_ai::request::BoxFuture<'static, Result<CreateAgentSessionRuntimeResult, JsString>>
        }) as pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeFactory
    };

    let runtime = create_agent_session_runtime(factory, &JsString::from(&cwd), &JsString::from(&cwd), session_manager)
        .await
        .expect("runtime");
    Harness {
        runtime,
        logs,
        extension_context,
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
    let mut harness = build_harness(false, false).await;
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
    let mut harness = build_harness(false, false).await;
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
    let mut harness = build_harness(true, true).await;
    let session_id = harness.runtime.session().session_id();

    let switched = harness
        .runtime
        .switch_session(&JsString::from("/does/not/exist.jsonl"), None, None)
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
    let mut harness = build_harness(false, false).await;
    let error = harness
        .runtime
        .fork(&JsString::from("missing"), ForkPosition::Before, None)
        .await
        .expect_err("invalid entry");
    assert_eq!(error, "Invalid entry ID for forking");
}

#[tokio::test]
async fn relocate_updates_cwd_and_appends_a_context_note() {
    let mut harness = build_harness(false, false).await;
    let target = tempfile::tempdir().expect("target dir");
    let target_cwd = target.path().display().to_string();
    harness
        .runtime
        .relocate(&JsString::from(&target_cwd))
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
    let mut harness = build_harness(false, false).await;
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
    let mut harness = build_harness(false, false).await;
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
    let mut harness = build_harness(false, false).await;
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
    let mut harness = build_harness(false, false).await;
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
        .switch_session(&source_file, Some(&JsString::from(&source_cwd)), None)
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
            .set_session_name(Some(&JsString::from("renamed-by-extension")))
            .expect("set session name");
    }
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
        _temp,
    } = build_harness(false, false).await;
    let runtime = Arc::new(tokio::sync::Mutex::new(runtime));
    AgentSessionRuntime::bind_session_runtime_actions(&runtime).await;

    let session = runtime.lock().await.session().clone();
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
    let session = runtime.lock().await.session().clone();
    let context = session.extension_runner().create_context();
    context
        .relocate(&JsString::from(&first))
        .expect("relocate handler is bound")
        .await
        .expect("relocate");
    assert_eq!(*runtime.lock().await.cwd(), first);

    let second = _temp.path().join("second");
    std::fs::create_dir(&second).expect("second dir");
    let second = second.display().to_string();
    let session = runtime.lock().await.session().clone();
    let context = session.extension_runner().create_context();
    context
        .relocate(&JsString::from(&second))
        .expect("relocate handler is rebound after replacement")
        .await
        .expect("relocate after replacement");
    assert_eq!(*runtime.lock().await.cwd(), second);

    let third = _temp.path().join("third");
    std::fs::create_dir(&third).expect("third dir");
    let third = third.display().to_string();
    let session = runtime.lock().await.session().clone();
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
    assert_eq!(*runtime.lock().await.cwd(), third);
}

#[tokio::test]
async fn dispose_emits_session_shutdown_quit_before_invalidating_the_session() {
    let mut harness = build_harness(false, false).await;
    let invalidated = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&invalidated);
    harness.runtime.set_before_session_invalidate(Some(Arc::new(move || {
        flag.store(true, Ordering::SeqCst);
    })));

    harness.runtime.dispose().await;

    let logs = harness.logs.lock().unwrap().clone();
    assert!(logs.contains(&"shutdown:quit".to_owned()), "logs: {logs:?}");
    assert!(invalidated.load(Ordering::SeqCst));
}

#[cfg(unix)]
#[tokio::test]
async fn import_through_a_symlinked_session_dir_preserves_the_source_file() {
    let mut harness = build_harness(false, false).await;
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
    session.set_active_tools_by_name(&[]);
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
