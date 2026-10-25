//! Behavioral tests for `extensions/agents-core/src/session-factories.ts`
//! production child and attached session factories.

#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::types::AgentMessage;
use pi_ai::providers::faux::{FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions, faux_provider};
use pi_ai::request::BoxFuture;
use pi_coding_agent::bundled_extensions::agents_core::session_factories::{
    AgentSessionChildSession, ChildExtensionFactories, CreateChildSessionManager,
    ProductionAttachedSessionFactoryOptions, ProductionChildAgentSessionFactoryOptions, UnboundChildAgentSession,
    create_production_attached_session_factory, create_production_child_agent_session_factory,
    spawn_prompt_validation_message, validate_goal_objective,
};
use pi_coding_agent::bundled_extensions::agents_core::shared::{
    AttachedSessionDispatchInput, ChildAgentDispatchInput, ChildAgentSession, SpawnContext,
};
use pi_coding_agent::core::agent_session::{AgentSession, MultiAgentRuntimeRole};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::SessionStartReason;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::multi_agent_store::{
    AgentModel, AgentPermission, AgentSnapshot, AgentTranscriptMetadata, MultiAgentStore, MultiAgentStoreOptions,
};
use pi_coding_agent::core::resource_loader::{InlineExtension, StaticResourceLoader};
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_coding_agent::core::session_manager::{SessionManager, SharedSessionManager, lock_session_manager};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

fn faux() -> FauxProviderHandle {
    faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
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
    })
}

async fn faux_runtime(faux: &FauxProviderHandle) -> ModelRegistry {
    let mut data = pi_js_compat::js_json::JsObject::new();
    data.insert(JsString::from("faux"), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(data)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    runtime
}

struct TestEnv {
    _dir: tempfile::TempDir,
    cwd: String,
    agent_dir: String,
    control_db: String,
    session: AgentSession,
    session_manager: SharedSessionManager,
}

async fn test_env_with_settings(settings: Option<Value>) -> TestEnv {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let agent_dir = dir.path().join("agent").display().to_string();
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    if let Some(settings) = settings {
        std::fs::write(
            std::path::Path::new(&agent_dir).join("settings.json"),
            serde_json::to_string(&settings).expect("settings json"),
        )
        .expect("settings file");
    }
    let session_dir = dir.path().join("sessions").display().to_string();
    let control_db = dir.path().join("control.db").display().to_string();
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let mut manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(session_dir)), None)
        .expect("session manager");
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone().into()),
        agent_dir: Some(agent_dir.clone().into()),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(manager),
        ..Default::default()
    })
    .await
    .expect("parent session");
    let session_manager = result.session.session_manager_handle();
    TestEnv {
        _dir: dir,
        cwd,
        agent_dir,
        control_db,
        session: result.session,
        session_manager,
    }
}

async fn test_env() -> TestEnv {
    test_env_with_settings(None).await
}

fn user_message(text: JsString) -> AgentMessage {
    pi_js_compat::js_json::from_value(&json!({
        "role": "user",
        "content": text,
        "timestamp": 1
    }))
    .expect("user message")
}

fn append_user(session_manager: &SharedSessionManager, text: impl Into<JsString>) {
    lock_session_manager(session_manager)
        .append_message(user_message(text.into()))
        .expect("append user");
}

fn assistant_message(text: &str) -> pi_ai::types::AssistantMessage {
    let mut message = pi_ai::types::AssistantMessage::empty("faux", "faux-1", "faux", 2.0);
    message
        .content
        .push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
            text,
        )));
    message
}

fn append_assistant(session_manager: &SharedSessionManager, text: &str) {
    lock_session_manager(session_manager)
        .append_message(AgentMessage::Assistant(Box::new(assistant_message(text))))
        .expect("append assistant");
}

fn session_texts(session_manager: &SharedSessionManager) -> Vec<JsString> {
    lock_session_manager(session_manager)
        .build_session_context()
        .messages
        .iter()
        .map(message_text)
        .collect()
}

fn message_text(message: &AgentMessage) -> JsString {
    match message {
        AgentMessage::User(user) => match &user.content {
            pi_ai::types::UserContent::Text(text) => text.clone(),
            pi_ai::types::UserContent::Blocks(blocks) => JsString::join(
                blocks
                    .iter()
                    .filter_map(|block| match block {
                        pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
                        pi_ai::types::TextOrImageContent::Image(_) => None,
                    })
                    .collect::<Vec<_>>(),
                "\n",
            ),
        },
        AgentMessage::Assistant(assistant) => JsString::join(
            assistant
                .content
                .iter()
                .filter_map(|part| match part {
                    pi_ai::types::AssistantContent::Text(text) => Some(text.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            "\n",
        ),
        other => JsString::from(format!("{other:?}")),
    }
}

fn child_agent(env: &TestEnv, id: &str, display_name: &str) -> AgentSnapshot {
    AgentSnapshot {
        key_order: Vec::new(),
        id: id.to_owned().into(),
        parent_id: Some("main".to_owned().into()),
        display_name: display_name.to_owned().into(),
        agent_type: "explore".into(),
        origin: None,
        lifecycle: pi_coding_agent::core::multi_agent_store::AgentLifecycle::Running,
        revision: 0,
        created_at: "2026-06-21T00:00:00.000Z".to_owned().into(),
        updated_at: "2026-06-21T00:00:00.000Z".to_owned().into(),
        cwd: env.cwd.clone().into(),
        worktree: None,
        model: Some(AgentModel {
            provider_id: "faux".to_owned(),
            model_id: "faux-1".to_owned(),
            thinking_level: Some("high".to_owned()),
        }),
        account: None,
        permission: AgentPermission {
            policy: "on-request".into(),
            inherited_from: None,
            narrowed: true,
        },
        slot: None,
        transcript: None,
        event_stream: None,
        worker: None,
        detached: None,
        current_activity: None,
        last_activity: None,
        result: None,
        error: None,
        extra: Default::default(),
    }
}

/// Minimal `UnboundChildAgentSession` backed by the manager passed through options.
struct FakeChildSession {
    session_manager: SharedSessionManager,
    summary: String,
    events: Arc<Mutex<Vec<&'static str>>>,
    shutdown_error: Option<JsString>,
    dispose_error: Option<JsString>,
}

impl FakeChildSession {
    fn new(session_manager: SharedSessionManager, summary: &str) -> Self {
        Self {
            session_manager,
            summary: summary.to_owned(),
            events: Default::default(),
            shutdown_error: None,
            dispose_error: None,
        }
    }
}

impl ChildAgentSession for FakeChildSession {
    fn dispose(&self) -> BoxFuture<'static, Result<(), JsString>> {
        let events = Arc::clone(&self.events);
        let error = self.dispose_error.clone();
        Box::pin(async move {
            events.lock().unwrap().push("dispose");
            error.map_or(Ok(()), Err)
        })
    }
    fn messages(&self) -> Vec<Arc<AgentMessage>> {
        lock_session_manager(&self.session_manager)
            .build_session_context()
            .messages
            .into_iter()
            .map(Arc::new)
            .collect()
    }

    fn prompt(&self, text: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        let manager = Arc::clone(&self.session_manager);
        let summary = self.summary.clone();
        Box::pin(async move {
            append_user(&manager, &text);
            append_assistant(&manager, &summary);
            Ok(())
        })
    }

    fn transcript(&self) -> Option<AgentTranscriptMetadata> {
        None
    }
}

impl UnboundChildAgentSession for FakeChildSession {
    fn emit_session_shutdown(&self) -> BoxFuture<'static, Result<(), JsString>> {
        let events = Arc::clone(&self.events);
        let error = self.shutdown_error.clone();
        Box::pin(async move {
            events.lock().unwrap().push("shutdown");
            error.map_or(Ok(()), Err)
        })
    }
    fn bind_extensions(&self) -> BoxFuture<'static, Result<(), JsString>> {
        Box::pin(async { Ok(()) })
    }

    fn session_manager(&self) -> SharedSessionManager {
        Arc::clone(&self.session_manager)
    }
}

#[derive(Default)]
struct Captured {
    options: Mutex<Option<CreateAgentSessionOptions>>,
    manager: Mutex<Option<SharedSessionManager>>,
    calls: Mutex<usize>,
    events: Arc<Mutex<Vec<&'static str>>>,
    shutdown_error: Mutex<Option<JsString>>,
    dispose_error: Mutex<Option<JsString>>,
}

type TestCreateSession = Arc<
    dyn Fn(CreateAgentSessionOptions) -> BoxFuture<'static, Result<Arc<dyn UnboundChildAgentSession>, JsString>>
        + Send
        + Sync,
>;

fn create_child_session(captured: Arc<Captured>, summary: &'static str) -> TestCreateSession {
    Arc::new(move |options: CreateAgentSessionOptions| {
        *captured.calls.lock().expect("calls") += 1;
        let manager = Arc::new(Mutex::new(options.session_manager.clone().expect("session manager")));
        *captured.manager.lock().expect("manager") = Some(Arc::clone(&manager));
        *captured.options.lock().expect("options") = Some(options);
        let events = Arc::clone(&captured.events);
        let shutdown_error = captured.shutdown_error.lock().unwrap().clone();
        let dispose_error = captured.dispose_error.lock().unwrap().clone();
        Box::pin(async move {
            let mut session = FakeChildSession::new(manager, summary);
            session.events = events;
            session.shutdown_error = shutdown_error;
            session.dispose_error = dispose_error;
            Ok(Arc::new(session) as Arc<dyn UnboundChildAgentSession>)
        })
    })
}

fn create_session_manager() -> CreateChildSessionManager {
    Arc::new(|cwd: &JsString, session_dir: Option<&JsString>, options| {
        SessionManager::create(cwd, session_dir, Some(options)).map_err(|error| error.message().clone())
    })
}

fn child_options(
    env: &TestEnv,
    captured: Arc<Captured>,
    extension_factories: Option<ChildExtensionFactories>,
    store: Option<Arc<MultiAgentStore>>,
) -> ProductionChildAgentSessionFactoryOptions {
    ProductionChildAgentSessionFactoryOptions {
        browser_agent_config_root: None,
        agent_dir: Some(env.agent_dir.clone().into()),
        create_session: create_child_session(captured, "fresh child done"),
        create_session_manager: create_session_manager(),
        extension_factories,
        multi_agent_store: store,
        session_dir: None,
    }
}

fn child_input(env: &TestEnv, agent: AgentSnapshot, context: SpawnContext, prompt: &str) -> ChildAgentDispatchInput {
    ChildAgentDispatchInput {
        active_tool_call_id: None,
        agent,
        context,
        ctx: env.session.extension_runner().create_context(),
        prompt: prompt.to_owned().into(),
        signal: None,
    }
}

fn captured_session_manager(captured: &Captured) -> SharedSessionManager {
    captured
        .manager
        .lock()
        .expect("manager")
        .clone()
        .expect("session manager")
}

#[tokio::test]
async fn production_bound_child_exposes_the_live_model_and_effort_surface() {
    let env = test_env().await;
    let mut options = child_options(&env, Arc::new(Captured::default()), None, None);
    options.create_session = Arc::new(|mut options| {
        options.resource_loader = Some(StaticResourceLoader::new());
        Box::pin(async move {
            let faux = faux();
            let registry = faux_runtime(&faux).await;
            registry.refresh().unwrap();
            options.model_registry = Some(registry);
            let created = create_agent_session(options).await.map_err(|error| error.message)?;
            Ok(Arc::new(
                pi_coding_agent::bundled_extensions::agents_core::session_factories::AgentSessionChildSession::new(
                    created.session,
                ),
            ) as Arc<dyn UnboundChildAgentSession>)
        })
    });
    let factory = create_production_child_agent_session_factory(options);
    let child = factory
        .call(child_input(
            &env,
            child_agent(&env, "child", "worker"),
            SpawnContext::Fresh,
            "task",
        ))
        .await
        .unwrap();
    let target = child
        .session_mutation_target()
        .expect("bound child must retain the live mutation surface");
    assert_eq!(target.model().unwrap().id, "faux-1");
    target
        .set_thinking_level(pi_agent_core::harness::ThinkingLevel::High)
        .unwrap();
    assert_eq!(target.thinking_level(), pi_agent_core::harness::ThinkingLevel::High);
    assert!(target.model_registry().find("faux", "faux-1").is_some());
    assert!(
        target
            .cycle_model(pi_coding_agent::core::agent_session::CycleDirection::Forward)
            .await
            .unwrap()
            .is_none()
    );
    child.dispose().await.unwrap();
}

#[tokio::test]
async fn bound_child_dispose_emits_shutdown_before_releasing_the_session() {
    let mut ts = oracle::TsOracle::spawn(
        "agents-core-repin-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    for (shutdown_error, dispose_error) in [
        (None, None),
        (Some("shutdown failed"), None),
        (None, Some("dispose failed")),
        (Some("shutdown failed"), Some("dispose failed")),
    ] {
        let source = ts.request(
            &json!({"id":5,"op":"childSessionDisposal","shutdownError":shutdown_error,"disposeError":dispose_error}),
        );
        assert_eq!(source["ok"], json!(true), "{source:?}");
        let env = test_env().await;
        let captured = Arc::new(Captured::default());
        *captured.shutdown_error.lock().unwrap() = shutdown_error.map(JsString::from);
        *captured.dispose_error.lock().unwrap() = dispose_error.map(JsString::from);
        let factory =
            create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None));
        let child = factory
            .call(child_input(
                &env,
                child_agent(&env, "child", "worker"),
                SpawnContext::Fresh,
                "task",
            ))
            .await
            .unwrap();
        let error = child.dispose().await.err();
        assert_eq!(
            source["output"],
            json!({"events":*captured.events.lock().unwrap(),"error":error})
        );
    }
}

#[tokio::test]
async fn factory_options_match_frozen_parent_registry_tools_and_settings() {
    let mut ts = oracle::TsOracle::spawn(
        "agents-core-repin-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    for attached in [false, true] {
        for agent_type in ["explore", "browser"] {
            let env =
                test_env_with_settings(Some(json!({"agents": {agent_type: {"tools": ["grep", "end_turn"]}}}))).await;
            let captured = Arc::new(Captured::default());
            let mut agent = child_agent(&env, "child", "worker");
            agent.agent_type = agent_type.into();
            agent.model = None;
            let session_dir = lock_session_manager(&env.session_manager).get_session_dir().clone();
            append_user(&env.session_manager, "existing");
            lock_session_manager(&env.session_manager)
                .persist_for_recovery()
                .unwrap();
            let session_path = lock_session_manager(&env.session_manager)
                .get_session_file()
                .cloned()
                .unwrap();
            let source = ts.request(&json!({
                "id": 20, "op": "factoryOptions", "agentType": agent_type,
                "cwd": env.cwd, "agentDir": env.agent_dir, "sessionDir": session_dir,
                "controlDbPath": env.control_db, "sessionPath": session_path,
                "tools": ["grep", "end_turn"], "attached": attached,
            }));
            assert_eq!(source["ok"], json!(true), "{source:?}");
            if attached {
                create_production_attached_session_factory(ProductionAttachedSessionFactoryOptions {
                    browser_agent_config_root: None,
                    agent_dir: Some(env.agent_dir.clone().into()),
                    create_session: create_child_session(Arc::clone(&captured), "done"),
                    extension_factories: None,
                    multi_agent_store: None,
                    session_dir: None,
                })
                .call(AttachedSessionDispatchInput {
                    active_tool_call_id: None,
                    agent,
                    ctx: env.session.extension_runner().create_context(),
                    prompt: "task".into(),
                    signal: None,
                    session_path,
                })
                .await
                .unwrap();
            } else {
                create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None))
                    .call(child_input(&env, agent, SpawnContext::Fresh, "task"))
                    .await
                    .unwrap();
            }
            let options = captured.options.lock().unwrap().take().unwrap();
            let native = json!({
                "excludeTools": options.exclude_tools,
                "tools": options.tools,
                "customTools": options.custom_tool_definitions.iter().map(|tool| tool.name.clone()).collect::<Vec<_>>(),
                "parentRegistry": options.model_registry.as_ref().is_some_and(|registry| registry.find("faux", "faux-1").is_some()),
                "settingsOverride": options.settings_manager.is_some(),
            });
            assert_eq!(native, source["output"], "attached={attached}, type={agent_type}");
        }
    }
}

#[tokio::test]
async fn fresh_child_gets_its_own_session_with_only_the_assignment() {
    let env = test_env().await;
    append_user(&env.session_manager, "Parent-only context marker");
    append_assistant(&env.session_manager, "Parent-only assistant marker");
    let parent_messages_before = session_texts(&env.session_manager);
    let parent_session_id = lock_session_manager(&env.session_manager).get_session_id().to_owned();
    let parent_session_file = lock_session_manager(&env.session_manager).get_session_file().cloned();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let captured = Arc::new(Captured::default());
    let factory = create_production_child_agent_session_factory(child_options(
        &env,
        Arc::clone(&captured),
        None,
        Some(Arc::clone(&store)),
    ));
    let agent = child_agent(&env, "agent_child", "Explorer");

    let child = factory
        .call(child_input(&env, agent, SpawnContext::Fresh, "Handle isolated task"))
        .await
        .expect("child session");
    child
        .prompt("Handle isolated task".to_owned().into())
        .await
        .expect("prompt");

    {
        let captured_options = captured.options.lock().expect("options");
        let options = captured_options.as_ref().expect("options");
        assert_eq!(options.cwd.clone(), Some(JsString::from(&env.cwd)));
        assert_eq!(options.agent_dir.clone(), Some(JsString::from(&env.agent_dir)));
        assert_eq!(
            options.multi_agent_agent_id.clone(),
            Some(JsString::from("agent_child"))
        );
        assert_eq!(
            options.multi_agent_parent_session_id.clone(),
            Some(parent_session_id.clone())
        );
        assert!(options.multi_agent_requires_agent_id);
        assert_eq!(options.multi_agent_runtime_role, Some(MultiAgentRuntimeRole::Child));
        assert!(options.multi_agent_store.is_some());
        assert_eq!(
            options.exclude_tools.clone().expect("exclude tools"),
            vec![
                "agent_viewer",
                "answer_question",
                "attach_session_agent",
                "close_agent",
                "list_agents",
                "set_agent_model",
                "spawn_agent",
                "steer_agent",
                "wait_agent",
                "ask_supervisor",
                "manage_goal",
                "change_working_directory",
            ]
        );
        assert_eq!(
            options.thinking_level,
            Some(pi_agent_core::harness::ThinkingLevel::High)
        );
        assert_eq!(options.model.as_ref().map(|model| model.id.as_str()), Some("faux-1"));
        let event = options.session_start_event.clone().expect("session start event");
        assert_eq!(event.reason, SessionStartReason::Fork);
        assert_eq!(event.previous_session_file, parent_session_file);
    }

    let child_manager = captured_session_manager(&captured);
    assert_eq!(
        session_texts(&child_manager),
        vec!["Handle isolated task".to_owned(), "fresh child done".to_owned()]
    );
    let child_id = lock_session_manager(&child_manager).get_session_id().to_owned();
    let child_file = lock_session_manager(&child_manager).get_session_file().cloned();
    assert_ne!(child_id, parent_session_id);
    assert_ne!(child_file, parent_session_file);
    assert_eq!(session_texts(&env.session_manager), parent_messages_before);
    assert_eq!(*captured.calls.lock().expect("calls"), 1);
}

#[tokio::test]
async fn inherited_child_forks_the_parent_prefix() {
    let env = test_env().await;
    append_user(&env.session_manager, "Inherited parent context marker");
    append_assistant(&env.session_manager, "Inherited parent assistant marker");
    let parent_messages_before = session_texts(&env.session_manager);
    let parent_session_file = lock_session_manager(&env.session_manager)
        .get_session_file()
        .cloned()
        .expect("persisted parent");
    let captured = Arc::new(Captured::default());
    let factory = create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None));
    let agent = child_agent(&env, "agent_child", "Explorer");

    let child = factory
        .call(child_input(
            &env,
            agent,
            SpawnContext::Inherit,
            "Continue integrated task",
        ))
        .await
        .expect("child session");
    let previous_messages = child.messages();
    assert_eq!(
        pi_coding_agent::bundled_extensions::agents_core::child_response::read_current_child_assistant_text(
            &child.messages(),
            &previous_messages
        )
        .unwrap(),
        None
    );
    child
        .prompt("Continue integrated task".to_owned().into())
        .await
        .expect("prompt");
    assert_eq!(
        pi_coding_agent::bundled_extensions::agents_core::child_response::read_current_child_assistant_text(
            &child.messages(),
            &previous_messages
        )
        .unwrap(),
        Some("fresh child done".into())
    );

    let child_manager = captured_session_manager(&captured);
    assert_eq!(
        session_texts(&child_manager),
        vec![
            "Inherited parent context marker".to_owned(),
            "Inherited parent assistant marker".to_owned(),
            "Continue integrated task".to_owned(),
            "fresh child done".to_owned(),
        ]
    );
    let parent_header = {
        let manager = lock_session_manager(&child_manager);
        manager.get_header().and_then(|header| header.parent_session().cloned())
    };
    assert_eq!(parent_header, Some(parent_session_file));
    assert_ne!(
        lock_session_manager(&child_manager).get_session_id(),
        lock_session_manager(&env.session_manager).get_session_id()
    );
    assert_eq!(session_texts(&env.session_manager), parent_messages_before);
}

#[tokio::test]
async fn inherited_child_copies_only_the_active_tool_call_branch() {
    let env = test_env().await;
    append_user(&env.session_manager, "Shared branch prefix");
    let active_leaf = lock_session_manager(&env.session_manager)
        .append_message(AgentMessage::Assistant(Box::new(assistant_message(
            "Active branch response",
        ))))
        .expect("active leaf");
    lock_session_manager(&env.session_manager)
        .branch(&active_leaf)
        .expect("branch active");
    append_user(&env.session_manager, "Later branch only");
    append_assistant(&env.session_manager, "Later branch response");
    lock_session_manager(&env.session_manager)
        .branch(&active_leaf)
        .expect("branch back");
    let tool_entry = lock_session_manager(&env.session_manager)
        .append_message(assistant_with_tool_call("spawn_agent", "spawn_agent-call"))
        .expect("tool call entry");
    let captured = Arc::new(Captured::default());
    let factory = create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None));
    let agent = child_agent(&env, "agent_child", "Explorer");
    let mut input = child_input(&env, agent, SpawnContext::Inherit, "Continue active branch");
    input.active_tool_call_id = Some("spawn_agent-call".to_owned());

    let child = factory.call(input).await.expect("child session");
    child
        .prompt("Continue active branch".to_owned().into())
        .await
        .expect("prompt");

    let child_manager = captured_session_manager(&captured);
    let texts = session_texts(&child_manager);
    assert_eq!(
        texts,
        vec![
            "Shared branch prefix".to_owned(),
            "Active branch response".to_owned(),
            "Continue active branch".to_owned(),
            "fresh child done".to_owned(),
        ]
    );
    assert!(!texts.contains(&JsString::from("Later branch only")));
    let branch = lock_session_manager(&child_manager).get_branch(None);
    assert!(
        branch.iter().all(|entry| entry.id() != Some(&tool_entry)),
        "active spawn call must not be copied"
    );
}

#[tokio::test]
async fn inherited_child_rejects_an_unpersisted_parent() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().display().to_string();
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(None).expect("model");
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone().into()),
        agent_dir: Some(cwd.clone().into()),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(SessionManager::in_memory(Some(&JsString::from(&cwd)), None).expect("in-memory")),
        control_db_path: Some(dir.path().join("control.db").display().to_string().into()),
        ..Default::default()
    })
    .await
    .expect("parent session");
    let session_manager = result.session.session_manager_handle();
    let env = TestEnv {
        _dir: dir,
        cwd: cwd.clone(),
        agent_dir: cwd.clone(),
        control_db: String::new(),
        session: result.session,
        session_manager,
    };
    let captured = Arc::new(Captured::default());
    let factory = create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None));
    let error = factory
        .call(child_input(
            &env,
            child_agent(&env, "agent_child", "Explorer"),
            SpawnContext::Inherit,
            "Continue integrated task",
        ))
        .await
        .err()
        .expect("unpersisted parent");
    assert_eq!(error, "Cannot inherit context from an unpersisted parent session");
    assert_eq!(*captured.calls.lock().expect("calls"), 0);
}

#[tokio::test]
async fn invalid_prompts_fail_before_session_creation() {
    let env = test_env().await;
    let captured = Arc::new(Captured::default());
    let factory = create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None));

    let empty = factory
        .call(child_input(
            &env,
            child_agent(&env, "agent_child", "Explorer"),
            SpawnContext::Fresh,
            "   ",
        ))
        .await
        .err()
        .expect("empty prompt");
    assert_eq!(empty, "spawn_agent requires a non-empty prompt");

    let long = "x".repeat(10_001);
    let too_long = factory
        .call(child_input(
            &env,
            child_agent(&env, "agent_child", "Explorer"),
            SpawnContext::Fresh,
            &long,
        ))
        .await
        .err()
        .expect("long prompt");
    assert_eq!(too_long, "spawn_agent prompt too long (10001 > 10000 chars)");
    assert_eq!(*captured.calls.lock().expect("calls"), 0);

    let validation = validate_goal_objective("   ");
    assert_eq!(JsString::from(spawn_prompt_validation_message(&validation)), empty);
}

#[tokio::test]
async fn active_tool_call_identity_must_be_non_empty() {
    for context in [SpawnContext::Inherit, SpawnContext::Fresh] {
        let env = test_env().await;
        append_user(&env.session_manager, "parent");
        let captured = Arc::new(Captured::default());
        let factory =
            create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None));
        let mut input = child_input(&env, child_agent(&env, "agent_child", "Explorer"), context, "Continue");
        input.active_tool_call_id = Some("  ".to_owned());
        let error = factory.call(input).await.err().expect("blank tool call id");
        assert_eq!(
            error,
            "Cannot inherit context: active tool call identity must be non-empty"
        );
    }
}

#[tokio::test]
async fn inherited_child_rejects_an_active_tool_call_outside_the_parent_branch() {
    let env = test_env().await;
    append_user(&env.session_manager, "Parent prefix");
    let captured = Arc::new(Captured::default());
    let factory = create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None));
    let mut input = child_input(
        &env,
        child_agent(&env, "agent_child", "Explorer"),
        SpawnContext::Inherit,
        "Continue",
    );
    input.active_tool_call_id = Some("missing-call".to_owned());
    let error = factory.call(input).await.err().expect("missing tool call");
    assert_eq!(
        error,
        "Cannot inherit context: active tool call missing-call is not in the parent branch"
    );
}

#[tokio::test]
async fn goal_extension_factory_is_filtered_from_child_sessions() {
    let env = test_env().await;
    let captured = Arc::new(Captured::default());
    let goal = InlineExtension::Factory {
        extension_path: Some("<first-party:goal>".to_owned().into()),
        factory: Arc::new(|_pi| Box::pin(async { Ok(()) })),
    };
    let probe = InlineExtension::Factory {
        extension_path: Some("<first-party:probe>".to_owned().into()),
        factory: Arc::new(|_pi| Box::pin(async { Ok(()) })),
    };
    let factory = create_production_child_agent_session_factory(child_options(
        &env,
        Arc::clone(&captured),
        Some(ChildExtensionFactories::Static(vec![goal, probe])),
        None,
    ));
    factory
        .call(child_input(
            &env,
            child_agent(&env, "agent_child", "Explorer"),
            SpawnContext::Fresh,
            "Run",
        ))
        .await
        .expect("child session");
    let captured_options = captured.options.lock().expect("options");
    let options = captured_options.as_ref().expect("options");
    let paths: Vec<Option<&JsString>> = options
        .extension_factories
        .iter()
        .map(InlineExtension::explicit_extension_path)
        .collect();
    assert_eq!(paths, vec![Some(&JsString::from("<first-party:probe>"))]);
}

#[tokio::test]
async fn child_agent_tools_append_control_tools_in_order() {
    let env = test_env_with_settings(Some(json!({
        "agents": {
            "explore": {"tools": ["read", "end_turn", "grep"]}
        }
    })))
    .await;
    let captured = Arc::new(Captured::default());
    let factory = create_production_child_agent_session_factory(child_options(&env, Arc::clone(&captured), None, None));
    let mut agent = child_agent(&env, "agent_child", "Explorer");
    agent.agent_type = "explore".to_owned().into();
    factory
        .call(child_input(&env, agent, SpawnContext::Fresh, "Run"))
        .await
        .expect("child session");
    let captured_options = captured.options.lock().expect("options");
    assert_eq!(
        captured_options
            .as_ref()
            .expect("options")
            .tools
            .clone()
            .expect("tools"),
        vec![
            "read".to_owned(),
            "end_turn".to_owned(),
            "grep".to_owned(),
            "ask_questions".to_owned(),
            "contact_parent".to_owned(),
            "send_agent_message".to_owned(),
        ]
    );
}

fn assistant_with_tool_call(name: &str, id: &str) -> AgentMessage {
    let mut message = pi_ai::types::AssistantMessage::empty("faux", "faux-1", "faux", 2.0);
    message
        .content
        .push(pi_ai::types::AssistantContent::ToolCall(pi_ai::types::ToolCall::new(
            id,
            name,
            Default::default(),
        )));
    message.stop_reason = pi_ai::types::StopReason::ToolUse;
    AgentMessage::Assistant(Box::new(message))
}

#[tokio::test]
async fn attached_session_opens_the_transcript_without_overriding_settings() {
    let env = test_env().await;
    let target_dir = env._dir.path().join("target").display().to_string();
    std::fs::create_dir_all(&target_dir).expect("target dir");
    let target_session_dir = env._dir.path().join("target-sessions").display().to_string();
    let mut target = SessionManager::create(
        &JsString::from(&target_dir),
        Some(&JsString::from(target_session_dir)),
        None,
    )
    .expect("target session");
    target
        .append_message(user_message(JsString::from("existing")))
        .expect("append target");
    target.persist_for_recovery().expect("persist target");
    let target_path = target.get_session_file().expect("target file").to_owned();
    let target_id = target.get_session_id().to_owned();
    let parent_session_file = lock_session_manager(&env.session_manager).get_session_file().cloned();
    let captured = Arc::new(Captured::default());
    let factory = create_production_attached_session_factory(ProductionAttachedSessionFactoryOptions {
        browser_agent_config_root: None,
        agent_dir: Some(env.agent_dir.clone().into()),
        create_session: create_child_session(Arc::clone(&captured), "attached done"),
        extension_factories: None,
        multi_agent_store: None,
        session_dir: None,
    });
    let mut agent = child_agent(&env, "agent_attached", "Attached");
    agent.cwd = target_dir.into();
    agent.permission.policy = "never".to_owned().into();
    agent.transcript = Some(AgentTranscriptMetadata {
        session_id: target_id.clone(),
        path: Some(target_path.clone()),
    });

    let child = factory
        .call(AttachedSessionDispatchInput {
            active_tool_call_id: None,
            agent,
            ctx: env.session.extension_runner().create_context(),
            prompt: "resume".to_owned().into(),
            signal: None,
            session_path: target_path.clone(),
        })
        .await
        .expect("attached session");
    child.prompt("continue".to_owned().into()).await.expect("prompt");

    let options = captured.options.lock().expect("options").take().expect("options");
    let manager = captured_session_manager(&captured);
    assert_eq!(lock_session_manager(&manager).get_session_id(), &target_id);
    assert_eq!(
        session_texts(&manager),
        vec!["existing".to_owned(), "continue".to_owned(), "attached done".to_owned()]
    );
    let event = options.session_start_event.clone().expect("session start event");
    assert_eq!(event.reason, SessionStartReason::Resume);
    assert_eq!(event.previous_session_file, parent_session_file);
    assert!(
        options.settings_manager.is_none(),
        "frozen factory leaves settings to SDK"
    );
    let control_db = lock_session_manager(&manager).get_metadata_control_db_path().cloned();
    assert_eq!(control_db.clone(), Some(JsString::from(&env.control_db)));
}

#[tokio::test]
async fn attached_session_rejects_missing_paths_and_session_mismatches() {
    let env = test_env().await;
    let captured = Arc::new(Captured::default());
    let factory = create_production_attached_session_factory(ProductionAttachedSessionFactoryOptions {
        browser_agent_config_root: None,
        agent_dir: Some(env.agent_dir.clone().into()),
        create_session: create_child_session(Arc::clone(&captured), "attached done"),
        extension_factories: None,
        multi_agent_store: None,
        session_dir: None,
    });
    let input = |session_path: &str, agent: AgentSnapshot| AttachedSessionDispatchInput {
        active_tool_call_id: None,
        agent,
        ctx: env.session.extension_runner().create_context(),
        prompt: "resume".to_owned().into(),
        signal: None,
        session_path: session_path.to_owned().into(),
    };

    let error = factory
        .call(input("", child_agent(&env, "agent_attached", "Attached")))
        .await
        .err()
        .expect("empty path");
    assert_eq!(error, "Cannot resume attached session without a session path");

    let missing = env._dir.path().join("missing.jsonl").display().to_string();
    let error = factory
        .call(input(&missing, child_agent(&env, "agent_attached", "Attached")))
        .await
        .err()
        .expect("missing path");
    assert_eq!(error, format!("Child agent transcript does not exist: {missing}"));

    let target_dir = env._dir.path().join("target").display().to_string();
    std::fs::create_dir_all(&target_dir).expect("target dir");
    let target_session_dir = env._dir.path().join("target-sessions").display().to_string();
    let mut target = SessionManager::create(
        &JsString::from(&target_dir),
        Some(&JsString::from(target_session_dir)),
        None,
    )
    .expect("target session");
    target
        .append_message(user_message(JsString::from("existing")))
        .expect("append target");
    target.persist_for_recovery().expect("persist target");
    let target_path = target.get_session_file().expect("target file").to_owned();
    let target_id = target.get_session_id().to_owned();
    let mut agent = child_agent(&env, "agent_attached", "Attached");
    agent.cwd = target_dir.into();
    agent.transcript = Some(AgentTranscriptMetadata {
        session_id: "different-session".to_owned().into(),
        path: Some(target_path.clone()),
    });
    let error = factory
        .call(input(&target_path.to_utf8().expect("utf-8 test path"), agent))
        .await
        .err()
        .expect("mismatch");
    assert_eq!(
        error,
        JsString::join(
            [
                JsString::from("Child agent transcript session mismatch: expected different-session, found "),
                target_id
            ],
            ""
        )
    );
    assert_eq!(*captured.calls.lock().expect("calls"), 0);
}

#[tokio::test]
async fn attached_factory_restores_archived_transcript_before_opening() {
    use pi_coding_agent::core::session_archive_storage::archive_session_file;
    let env = test_env().await;
    let mut target = SessionManager::create(&env.cwd.clone().into(), None, None).unwrap();
    target
        .append_message(user_message("archived conversation".into()))
        .unwrap();
    target.persist_for_recovery().unwrap();
    let original = target.get_session_file().cloned().unwrap();
    let archived = archive_session_file(&original).unwrap();
    let captured = Arc::new(Captured::default());
    let factory = create_production_attached_session_factory(ProductionAttachedSessionFactoryOptions {
        browser_agent_config_root: None,
        agent_dir: Some(env.agent_dir.clone().into()),
        create_session: create_child_session(Arc::clone(&captured), "done"),
        extension_factories: None,
        multi_agent_store: None,
        session_dir: None,
    });
    let child = factory
        .call(AttachedSessionDispatchInput {
            active_tool_call_id: None,
            agent: child_agent(&env, "restored", "worker"),
            ctx: env.session.extension_runner().create_context(),
            prompt: "resume".into(),
            signal: None,
            session_path: archived.clone(),
        })
        .await
        .unwrap();
    assert_eq!(child.transcript().unwrap().path, Some(original.clone()));
    assert!(std::path::Path::new(&original.to_utf8().unwrap()).exists());
    assert!(!std::path::Path::new(&archived.to_utf8().unwrap()).exists());
    assert_eq!(
        session_texts(&captured_session_manager(&captured)),
        vec![JsString::from("archived conversation")]
    );
}

#[tokio::test]
async fn attach_path_uses_control_metadata_restores_archive_and_keeps_on_request_policy() {
    use pi_coding_agent::bundled_extensions::agents_core::runtime::{
        MultiAgentExtensionOptions, create_multi_agent_pi_request_handler,
    };
    use pi_coding_agent::core::session_archive_storage::archive_session_file;
    for archived in [false, true] {
        let env = test_env_with_settings(Some(json!({"approvalPolicy":"never"}))).await;
        let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
        lock_session_manager(&env.session_manager)
            .persist_for_recovery()
            .unwrap();
        store.set_persistence_session_manager(Some(&*lock_session_manager(&env.session_manager)));
        let mut target = SessionManager::create(&env.cwd.clone().into(), None, None).unwrap();
        target
            .set_metadata_control_db_path(Some(&env.control_db.clone().into()), Default::default())
            .unwrap();
        target.persist_for_recovery().unwrap();
        target.set_session_name(Some(&"Metadata 🦀".into())).unwrap();
        let path = target.get_session_file().cloned().unwrap();
        let input = if archived {
            archive_session_file(&path).unwrap()
        } else {
            path.clone()
        };
        let handler = create_multi_agent_pi_request_handler(&MultiAgentExtensionOptions {
            store: Some(store),
            ..Default::default()
        });
        let details = handler
            .handle(
                &json!({"method":"agents.attachSession","params":{"path":input}}),
                &env.session.extension_runner().create_context(),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(details["agent"]["displayName"], json!("Metadata 🦀"));
        assert_eq!(details["agent"]["permission"]["policy"], json!("on-request"));
        assert_eq!(details["agent"]["transcript"]["path"], json!(path));
    }
}

/// A real `AgentSession` wrapped by the adapter behaves like a child session.
#[tokio::test]
async fn agent_session_child_adapter_forwards_prompt_and_transcript_state() {
    let env = test_env().await;
    let adapter = AgentSessionChildSession::new(env.session.clone());
    assert!(adapter.transcript().is_none());
    let adapter_session_id = adapter
        .session_manager()
        .lock()
        .expect("manager")
        .get_session_id()
        .to_owned();
    let env_session_id = lock_session_manager(&env.session_manager).get_session_id().to_owned();
    assert_eq!(adapter_session_id, env_session_id);
    adapter.bind_extensions().await.expect("bind extensions");
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
