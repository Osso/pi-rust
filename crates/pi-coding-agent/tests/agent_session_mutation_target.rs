//! Behavioral tests for the viewed-session mutation target
//! (`core/agent-session.ts` `bindAgentSessionMutationTargetResolver` and the
//! agents-core `resolveSelectedSessionMutationTarget`).

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_ai::providers::faux::{FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions, faux_provider};
use pi_ai::request::BoxFuture;
use pi_coding_agent::bundled_extensions::agents_core::runtime::{
    create_multi_agent_runtime_handles, resolve_selected_session_mutation_target,
};
use pi_coding_agent::bundled_extensions::agents_core::session_factories::AgentSessionChildSession;
use pi_coding_agent::core::agent_session::{
    AgentSession, ExtensionBindings, PromptOptions, bind_agent_session_mutation_target_resolver,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{
    HandlerError, ViewedSessionMutationTarget, ViewedSessionMutationTargetResolver,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentPermission, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions,
};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_coding_agent::core::session_manager::SessionManager;
use serde_json::json;

struct TestMutationTarget {
    model: Option<pi_ai::types::Model>,
    model_registry: Arc<pi_coding_agent::core::model_registry::ModelRegistry>,
    scoped_models: Vec<pi_coding_agent::core::model_resolver::ScopedModel>,
    thinking_level: pi_agent_core::harness::types::ThinkingLevel,
    set_model: Arc<dyn Fn(pi_ai::types::Model) -> BoxFuture<'static, Result<bool, HandlerError>> + Send + Sync>,
    set_thinking_level:
        Arc<dyn Fn(pi_agent_core::harness::types::ThinkingLevel) -> Result<(), HandlerError> + Send + Sync>,
}

impl pi_coding_agent::core::extensions::types::SessionMutationTarget for TestMutationTarget {
    fn model(&self) -> Option<pi_ai::types::Model> {
        self.model.clone()
    }

    fn thinking_level(&self) -> pi_agent_core::harness::types::ThinkingLevel {
        self.thinking_level
    }

    fn set_model(&self, model: pi_ai::types::Model) -> BoxFuture<'static, Result<bool, HandlerError>> {
        (self.set_model)(model)
    }

    fn set_thinking_level(&self, level: pi_agent_core::harness::types::ThinkingLevel) -> Result<(), HandlerError> {
        (self.set_thinking_level)(level)
    }
}

impl pi_coding_agent::core::extensions::types::ViewedSessionMutationTarget for TestMutationTarget {
    fn cycle_model(
        &self,
        _direction: pi_coding_agent::core::agent_session::CycleDirection,
    ) -> BoxFuture<'static, Result<Option<pi_coding_agent::core::agent_session::ModelCycleResult>, HandlerError>> {
        Box::pin(async { panic!("this fixture does not cycle models") })
    }

    fn set_model_with_thinking_level(
        &self,
        _model: pi_ai::types::Model,
        _level: Option<pi_agent_core::harness::types::ThinkingLevel>,
    ) -> BoxFuture<'static, Result<bool, HandlerError>> {
        Box::pin(async { panic!("this selected-view fixture does not exercise model+effort selection") })
    }

    fn model_registry(&self) -> Arc<pi_coding_agent::core::model_registry::ModelRegistry> {
        Arc::clone(&self.model_registry)
    }

    fn scoped_models(&self) -> Vec<pi_coding_agent::core::model_resolver::ScopedModel> {
        self.scoped_models.clone()
    }
}

fn faux() -> FauxProviderHandle {
    faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![
            FauxModelDefinition {
                id: "faux-1".to_owned(),
                name: Some("Faux One".into()),
                reasoning: Some(true),
                input: None,
                cost: None,
                context_window: Some(100_000.0),
                max_tokens: Some(4096.0),
            },
            FauxModelDefinition {
                id: "faux-2".to_owned(),
                name: Some("Faux Two".into()),
                reasoning: Some(true),
                input: None,
                cost: None,
                context_window: Some(100_000.0),
                max_tokens: Some(4096.0),
            },
        ]),
        ..Default::default()
    })
}

async fn faux_runtime(faux: &FauxProviderHandle) -> ModelRegistry {
    let mut data = serde_json::Map::new();
    data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(data)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    runtime.refresh().unwrap();
    runtime
}

struct TestEnv {
    _dir: tempfile::TempDir,
    session: AgentSession,
    faux: FauxProviderHandle,
}

async fn test_env() -> TestEnv {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let agent_dir = dir.path().join("agent").display().to_string();
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    let session_dir = dir.path().join("sessions").display().to_string();
    let control_db = dir.path().join("control.db").display().to_string();
    let faux = faux();
    let runtime = faux_runtime(&faux).await;
    let model = faux.get_model(Some("faux-1")).expect("model");
    let mut manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
        .expect("session manager");
    manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(cwd.clone())),
        agent_dir: Some(JsString::from(agent_dir)),
        model_registry: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(manager),
        control_db_path: Some(JsString::from(control_db)),
        ..Default::default()
    })
    .await
    .expect("session");
    TestEnv {
        _dir: dir,
        session: result.session,
        faux,
    }
}

fn agent(id: &str, agent_type: &str, lifecycle: AgentLifecycle) -> AgentSnapshot {
    AgentSnapshot {
        key_order: Vec::new(),
        id: JsString::from(id.to_owned()),
        parent_id: Some(JsString::from("main")),
        display_name: JsString::from(format!("Agent {id}")),
        agent_type: agent_type.into(),
        origin: None,
        lifecycle,
        revision: 1,
        created_at: JsString::from("2026-06-21T00:00:00.000Z"),
        updated_at: JsString::from("2026-06-21T00:00:00.000Z"),
        cwd: JsString::from("/repo"),
        worktree: None,
        model: None,
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

fn store_with(agents: &[AgentSnapshot]) -> Arc<MultiAgentStore> {
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    for agent in agents {
        store.publish_lifecycle_coordinator_snapshot(agent).expect("seed agent");
    }
    store
}

fn error_text(result: Result<Option<Arc<dyn ViewedSessionMutationTarget>>, HandlerError>) -> JsString {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error.message,
    }
}

#[test]
fn resolve_selected_session_mutation_target_reports_source_errors() {
    let handles = create_multi_agent_runtime_handles();
    let store = store_with(&[
        agent("main", "main", AgentLifecycle::Running),
        agent("agent_bg", "background", AgentLifecycle::Running),
        agent("agent_done", "worker", AgentLifecycle::Completed),
        agent("agent_live", "worker", AgentLifecycle::Running),
    ]);

    // No selection and the main thread both resolve to no target.
    assert!(
        resolve_selected_session_mutation_target(&store, &handles)
            .expect("none")
            .is_none()
    );
    store.select_agent_view(&JsString::from("main"));
    assert!(
        resolve_selected_session_mutation_target(&store, &handles)
            .expect("main")
            .is_none()
    );

    store.select_agent_view(&JsString::from("agent_bg"));
    assert_eq!(
        error_text(resolve_selected_session_mutation_target(&store, &handles)),
        "Agent agent_bg is detached and not a live child session"
    );

    store.select_agent_view(&JsString::from("agent_done"));
    assert_eq!(
        error_text(resolve_selected_session_mutation_target(&store, &handles)),
        "Agent is not active: Agent agent_done (completed)"
    );

    store.select_agent_view(&JsString::from("agent_live"));
    assert_eq!(
        error_text(resolve_selected_session_mutation_target(&store, &handles)),
        "Agent agent_live is not a live child session"
    );
}

#[tokio::test]
async fn resolve_selected_session_mutation_target_requires_a_live_mutable_session() {
    let env = test_env().await;
    let handles = create_multi_agent_runtime_handles();
    let store = store_with(&[agent("agent_live", "worker", AgentLifecycle::Running)]);
    store.select_agent_view(&JsString::from("agent_live"));

    // A session without a mutation surface is rejected like the TS structural check.
    struct PlainSession;
    impl pi_coding_agent::bundled_extensions::agents_core::shared::ChildAgentSession for PlainSession {
        fn messages(&self) -> Vec<std::sync::Arc<pi_agent_core::agent::types::AgentMessage>> {
            Vec::new()
        }
        fn prompt(&self, _text: JsString) -> BoxFuture<'static, Result<(), JsString>> {
            Box::pin(async { Ok(()) })
        }
        fn transcript(&self) -> Option<pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata> {
            None
        }
    }
    handles
        .sessions
        .lock()
        .expect("sessions")
        .insert(JsString::from("agent_live"), Arc::new(PlainSession));
    assert_eq!(
        error_text(resolve_selected_session_mutation_target(&store, &handles)),
        "Agent agent_live does not support live session mutation"
    );

    let adapter = AgentSessionChildSession::new(env.session.clone());
    handles
        .sessions
        .lock()
        .expect("sessions")
        .insert(JsString::from("agent_live"), Arc::new(adapter));
    let target = resolve_selected_session_mutation_target(&store, &handles)
        .expect("target")
        .expect("target");
    assert_eq!(target.model().as_ref().map(|model| model.id.as_str()), Some("faux-1"));
    assert_eq!(
        target.thinking_level(),
        pi_agent_core::harness::types::ThinkingLevel::Medium
    );
    assert!(target.scoped_models().is_empty());
    assert_eq!(
        target.model_registry().find("faux", "faux-1").map(|model| model.id),
        Some("faux-1".to_owned())
    );
}

#[tokio::test]
async fn viewed_context_shadows_model_and_thinking_accessors() {
    let env = test_env().await;
    let base = env.session.extension_runner().create_context();
    assert_eq!(
        base.model().expect("model").map(|model| model.id),
        Some("faux-1".to_owned())
    );

    let set_model_calls = Arc::new(Mutex::new(Vec::new()));
    let set_thinking_calls = Arc::new(Mutex::new(Vec::new()));
    let target_model = env.faux.get_model(Some("faux-2")).expect("faux-2");
    let target: Arc<dyn ViewedSessionMutationTarget> = Arc::new(TestMutationTarget {
        model: Some(target_model),
        model_registry: Arc::new(env.session.model_registry().clone()),
        scoped_models: Vec::new(),
        thinking_level: pi_agent_core::harness::types::ThinkingLevel::High,
        set_model: {
            let calls = Arc::clone(&set_model_calls);
            Arc::new(move |model| {
                calls.lock().expect("calls").push(model.id.clone());
                Box::pin(async { Ok(true) })
            })
        },
        set_thinking_level: {
            let calls = Arc::clone(&set_thinking_calls);
            Arc::new(move |level| {
                calls.lock().expect("calls").push(level);
                Ok(())
            })
        },
    });
    let resolver: ViewedSessionMutationTargetResolver = Arc::new({
        let target = Arc::clone(&target);
        move || Ok(Some(Arc::clone(&target)))
    });
    let viewed = base.with_viewed_session_mutation_target(resolver);

    assert_eq!(
        viewed.model().expect("model").map(|model| model.id),
        Some("faux-2".to_owned())
    );
    assert_eq!(
        viewed.thinking_level().expect("thinking"),
        pi_agent_core::harness::types::ThinkingLevel::High
    );
    assert!(viewed.scoped_models().expect("scoped").is_empty());
    let requested = env.faux.get_model(Some("faux-2")).expect("faux-2");
    assert!(viewed.set_model(requested).await.expect("set model"));
    viewed
        .set_thinking_level(pi_agent_core::harness::types::ThinkingLevel::Low)
        .expect("set thinking");
    assert_eq!(*set_model_calls.lock().expect("calls"), vec!["faux-2".to_owned()]);
    assert_eq!(
        *set_thinking_calls.lock().expect("calls"),
        vec![pi_agent_core::harness::types::ThinkingLevel::Low]
    );
}

#[tokio::test]
async fn model_command_targets_the_viewed_session() {
    let env = test_env().await;
    let set_model_calls = Arc::new(Mutex::new(Vec::new()));
    let target_model = env.faux.get_model(Some("faux-2")).expect("faux-2");
    let target: Arc<dyn ViewedSessionMutationTarget> = Arc::new(TestMutationTarget {
        model: Some(target_model),
        model_registry: Arc::new(env.session.model_registry().clone()),
        scoped_models: Vec::new(),
        thinking_level: pi_agent_core::harness::types::ThinkingLevel::Off,
        set_model: {
            let calls = Arc::clone(&set_model_calls);
            Arc::new(move |model| {
                calls.lock().expect("calls").push(model.id.clone());
                Box::pin(async { Ok(true) })
            })
        },
        set_thinking_level: Arc::new(|_: pi_agent_core::harness::types::ThinkingLevel| Ok::<(), HandlerError>(())),
    });
    bind_agent_session_mutation_target_resolver(&env.session, Arc::new(move || Ok(Some(Arc::clone(&target)))));

    env.session
        .prompt("/model faux/faux-2", PromptOptions::default())
        .await
        .expect("model command");

    assert_eq!(*set_model_calls.lock().expect("calls"), vec!["faux-2".to_owned()]);
    assert_eq!(env.session.model().id, "faux-1");
}

#[tokio::test]
async fn model_command_reports_a_missing_api_key_for_the_viewed_session() {
    let env = test_env().await;
    let errors = Arc::new(Mutex::new(Vec::new()));
    env.session
        .bind_extensions(ExtensionBindings {
            on_error: Some(Arc::new({
                let errors = Arc::clone(&errors);
                move |error| errors.lock().expect("errors").push(error.error.clone())
            })),
            ..Default::default()
        })
        .await
        .expect("bind extensions");
    let target_model = env.faux.get_model(Some("faux-2")).expect("faux-2");
    let target: Arc<dyn ViewedSessionMutationTarget> = Arc::new(TestMutationTarget {
        model: Some(target_model),
        model_registry: Arc::new(env.session.model_registry().clone()),
        scoped_models: Vec::new(),
        thinking_level: pi_agent_core::harness::types::ThinkingLevel::Off,
        set_model: Arc::new(|_| Box::pin(async { Ok(false) })),
        set_thinking_level: Arc::new(|_: pi_agent_core::harness::types::ThinkingLevel| Ok::<(), HandlerError>(())),
    });
    bind_agent_session_mutation_target_resolver(&env.session, Arc::new(move || Ok(Some(Arc::clone(&target)))));

    env.session
        .prompt("/model faux/faux-2", PromptOptions::default())
        .await
        .expect("model command");

    assert_eq!(
        *errors.lock().expect("errors"),
        vec!["No API key for faux/faux-2".to_owned()]
    );
}

use crate::shared_scripted_registry as scripted_registry;

// Shared support is compiled once per integration target.
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
