//! Frozen agent-model-tool registration and live-child mutation contract.
use crate::shared_bundled as bundled;
use crate::shared_oracle as oracle;
use pi_js_compat::js_json::JsString;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, FauxResponseStep,
    RegisterFauxProviderOptions, faux_assistant_message, faux_provider,
};
use pi_coding_agent::bundled_extensions::agents_core::session_factories::AgentSessionChildSession;
use pi_coding_agent::core::agent_session::{AgentSession, PromptOptions};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::multi_agent_store::{AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::js_json as json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_coding_agent::bundled_extensions::agents_core::runtime::{
    MultiAgentExtensionOptions, create_multi_agent_runtime_handles, multi_agent_extension,
};
use pi_coding_agent::core::extensions::{Extension, create_extension_runtime, load_extension_from_factory};

async fn registered(options: MultiAgentExtensionOptions) -> Extension {
    load_extension_from_factory(
        Arc::new(move |api| {
            let result = multi_agent_extension(api, &options);
            Box::pin(async move { result })
        }),
        &JsString::from("/fixture"),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        &JsString::from("<first-party:agents-core>"),
    )
    .await
    .expect("agents-core factory")
}

fn agent(id: &str, lifecycle: &str, agent_type: &str) -> AgentSnapshot {
    pi_js_compat::js_json::from_value(&json!({
        "id": id, "parentId": "main", "displayName": "Worker", "agentType": agent_type,
        "lifecycle": lifecycle, "revision": 0, "createdAt": "2026-10-21T00:00:00.000Z",
        "updatedAt": "2026-10-21T00:00:00.000Z", "cwd": "/fixture",
        "permission": {"narrowed": true, "policy": "on-request"}
    }))
    .unwrap()
}

#[tokio::test]
async fn errors_and_order_match_frozen_registered_tool() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-model-tool-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts",
    );
    let agents = vec![
        agent("active", "running", "explore"),
        agent("done", "completed", "explore"),
        agent("detached", "completed", "background"),
    ];
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    for agent in &agents {
        store.publish_lifecycle_coordinator_snapshot(agent).unwrap();
    }
    store.select_agent_view(&JsString::from("active"));
    let extension = registered(MultiAgentExtensionOptions {
        store: Some(store.clone()),
        ..Default::default()
    })
    .await;
    let definition = extension
        .read()
        .tools
        .get("set_agent_model")
        .expect("registration")
        .definition
        .clone();
    let runtime = create_extension_runtime();
    let harness = bundled::Harness::new(vec![extension], runtime, bundled::HarnessOptions::new("/fixture")).await;
    for params in [
        json!({"agentId": "missing"}),
        json!({"agentId": "missing", "provider": "faux"}),
        json!({"agentId": "missing", "provider": "  ", "modelId": " \n"}),
        json!({"agentId": "missing", "thinkingLevel": "high"}),
        json!({"agentId": "done", "thinkingLevel": "high"}),
        json!({"agentId": "detached", "thinkingLevel": "high"}),
        json!({"agentId": "active", "thinkingLevel": "high"}),
    ] {
        let expected = oracle.request(&json!({"params": params, "agents": agents, "selected": "active"}));
        let error = (definition.execute)(
            "fixture".into(),
            params.clone(),
            None,
            None,
            Some(harness.runner.create_context()),
        )
        .await
        .unwrap_err();
        assert_eq!(
            json!({"error": error.message, "selected": store.get_selected_agent_id()}),
            expected,
            "{params:?}"
        );
    }
}

struct LiveFixture {
    _dir: tempfile::TempDir,
    child: AgentSession,
    faux: FauxProviderHandle,
    store: Arc<MultiAgentStore>,
    handles: pi_coding_agent::bundled_extensions::agents_core::runtime::MultiAgentRuntimeHandles,
    settings: Arc<Mutex<SettingsManager>>,
}

async fn live_fixture() -> LiveFixture {
    live_fixture_with_persistence(None).await
}

async fn live_fixture_with_persistence(persistence: Option<(&mut oracle::TsOracle, bool, &str)>) -> LiveFixture {
    let persisted = persistence.is_some();
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().display().to_string();
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".into()),
        models: Some(
            [("old", true), ("new", true), ("plain", false)]
                .map(|(id, reasoning)| FauxModelDefinition {
                    id: id.into(),
                    name: None,
                    reasoning: Some(reasoning),
                    input: None,
                    cost: None,
                    context_window: Some(100_000.0),
                    max_tokens: Some(4096.0),
                })
                .to_vec(),
        ),
        ..Default::default()
    });
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(
        serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
            serde_json::json!({"faux": {"type":"api_key", "key":"fixture"}}),
        )
        .unwrap(),
    ))
    .unwrap();
    scripted_registry::register_scripted_provider(&runtime, faux.provider.clone());
    runtime.refresh().unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    for snapshot in [
        agent("child", "running", "explore"),
        agent("viewed", "running", "explore"),
    ] {
        store.publish_lifecycle_coordinator_snapshot(&snapshot).unwrap();
    }
    if let Some((oracle, source_created, owner_session)) = persistence {
        seed_owned_store(oracle, source_created, owner_session, &dir, &store);
    }
    store.select_agent_view(&JsString::from("viewed"));
    let settings = Arc::new(Mutex::new(SettingsManager::in_memory(
        Default::default(),
        SettingsManagerCreateOptions::default(),
    )));
    let child = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone().into()),
        agent_dir: Some(cwd.clone().into()),
        model_registry: Some(runtime),
        model: faux.get_model(Some("old")),
        thinking_level: Some(ThinkingLevel::Low),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(SessionManager::in_memory(Some(&JsString::from(&cwd)), None).unwrap()),
        settings_manager: Some(settings.clone()),
        control_db_path: Some(dir.path().join("control.db").display().to_string().into()),
        multi_agent_store: Some(store.clone()),
        multi_agent_agent_id: Some("child".into()),
        multi_agent_parent_session_id: persisted.then(|| JsString::from("fixture-parent")),
        disable_runtime_coordination_inbound: true,
        ..Default::default()
    })
    .await
    .unwrap()
    .session;
    let handles = create_multi_agent_runtime_handles();
    handles
        .sessions
        .lock()
        .unwrap()
        .insert("child".into(), Arc::new(AgentSessionChildSession::new(child.clone())));
    LiveFixture {
        _dir: dir,
        child,
        faux,
        store,
        handles,
        settings,
    }
}

struct Persistence {
    db: String,
    session: String,
}
impl pi_coding_agent::core::multi_agent_store::SessionPersistence for Persistence {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.db.as_str().into())
    }
    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.session.as_str().into())
    }
}

fn seed_owned_store(
    oracle: &mut oracle::TsOracle,
    source_created: bool,
    owner_session: &str,
    dir: &tempfile::TempDir,
    store: &MultiAgentStore,
) {
    use pi_coding_agent::core::session_control_db as db;
    let path = dir.path().join("control.db").display().to_string();
    let session = dir.path().join("parent.session").display().to_string();
    let mut child = agent("child", "running", "explore");
    child.revision = 1;
    child.model = Some(pi_coding_agent::core::multi_agent_store::AgentModel {
        provider_id: "faux".into(),
        model_id: "old".into(),
        thinking_level: Some("low".into()),
    });
    let viewed = agent("viewed", "running", "explore");
    let identity: pi_coding_agent::core::runtime_process::ProcessIdentity =
        serde_json::from_str(db::get_runtime_process_instance_id().unwrap()).unwrap();
    if source_created {
        let result = oracle.request(
            &json!({"op":"create", "db":path, "session":session, "agents":[viewed], "input":{
                "sessionPath":session, "agentId":"child", "processIdentity":identity,
                "owner":{"sessionId":owner_session,"agentId":null}, "nowIso":child.updated_at, "agent":child
            }}),
        );
        assert_eq!(result["ok"], pi_js_compat::js_json::JsValue::Bool(true), "{result:?}");
    } else {
        db::bootstrap_multi_agent_agent(
            &path,
            &session,
            &JsString::from("viewed"),
            &pi_js_compat::js_json::to_value(&viewed).unwrap(),
        )
        .unwrap();
        let result = db::create_multi_agent_child_with_runtime_ownership(
            &path,
            &db::CreateMultiAgentChildWithRuntimeOwnershipInput {
                session_path: session.clone().into(),
                agent_id: "child".into(),
                process_identity: identity,
                owner: db::MultiAgentRuntimeOwnerIdentity {
                    session_id: owner_session.into(),
                    agent_id: None,
                },
                now_iso: child.updated_at.to_utf8().expect("fixture timestamp"),
                agent: child,
            },
        )
        .unwrap();
        assert!(matches!(
            result,
            db::CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { .. }
        ));
    }
    store
        .restore_from_session_manager(&Persistence { db: path, session })
        .unwrap();
}

#[tokio::test]
async fn live_request_keeps_old_model_next_request_uses_effective_model_and_effort() {
    let fixture = live_fixture().await;
    let before_settings =
        pi_js_compat::js_json::to_value(&fixture.settings.lock().unwrap().get_global_settings()).unwrap();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let observed = Arc::new(Mutex::new(Vec::new()));
    let first_signal = Arc::new(Mutex::new(None));
    let response = |hold: bool| {
        let started = started.clone();
        let release = release.clone();
        let observed = observed.clone();
        let first_signal = first_signal.clone();
        FauxResponseStep::Factory(Arc::new(move |_context, options, _state, model| {
            let started = started.clone();
            let release = release.clone();
            let observed = observed.clone();
            let first_signal = first_signal.clone();
            Box::pin(async move {
                let options = options.expect("request options");
                observed.lock().unwrap().push((model.id, options.reasoning));
                if hold {
                    *first_signal.lock().unwrap() = options.stream.request.signal.clone();
                    started.notify_one();
                    release.notified().await;
                }
                Ok(faux_assistant_message("done", FauxAssistantMessageOptions::default()))
            })
        }))
    };
    fixture.faux.set_responses(vec![response(true), response(false)]);
    let child = fixture.child.clone();
    let pending = tokio::spawn(async move { child.prompt("first", PromptOptions::default()).await });
    tokio::time::timeout(Duration::from_secs(5), started.notified())
        .await
        .unwrap();
    let extension = registered(MultiAgentExtensionOptions {
        store: Some(fixture.store.clone()),
        runtime_handles: Some(fixture.handles.clone()),
        ..Default::default()
    })
    .await;
    let definition = extension
        .read()
        .tools
        .get("set_agent_model")
        .unwrap()
        .definition
        .clone();
    let harness = bundled::Harness::new(
        vec![extension],
        create_extension_runtime(),
        bundled::HarnessOptions::new("/fixture"),
    )
    .await;
    let published = Arc::new(Mutex::new(Vec::new()));
    let published_for_listener = published.clone();
    let _subscription = fixture.store.subscribe_agent_updates(move |_previous, current| {
        if current.id == "child" {
            published_for_listener.lock().unwrap().push(current.model.clone());
        }
        Ok(())
    });
    let result = (definition.execute)(
        "change".into(),
        json!({"agentId":"child", "provider":" \u{feff}faux\u{a0}", "modelId":" new ", "thinkingLevel":"ultra"}),
        None,
        None,
        Some(harness.runner.create_context()),
    )
    .await
    .unwrap();
    assert_eq!(
        result.details,
        Some(json!({"agentId":"child","provider":"faux","modelId":"new","thinkingLevel":"high"}))
    );
    let publication_states = published.lock().unwrap().clone();
    assert!(!publication_states.is_empty(), "effective model must be published");
    assert!(
        publication_states
            .iter()
            .all(|model| pi_js_compat::js_json::to_value(model).unwrap()
                == json!({"providerId":"faux","modelId":"new","thinkingLevel":"high"})),
        "every published model must already contain effective clamped effort: {publication_states:?}"
    );
    assert!(!pending.is_finished(), "held request must remain in flight");
    assert!(
        !first_signal.lock().unwrap().as_ref().unwrap().aborted(),
        "setter cancelled held request"
    );
    assert_eq!(
        *observed.lock().unwrap(),
        vec![("old".into(), Some(pi_ai::types::ThinkingLevel::Low))]
    );
    assert_eq!(
        fixture.store.get_selected_agent_id().clone(),
        Some(JsString::from("viewed"))
    );
    assert_eq!(
        pi_js_compat::js_json::to_value(&fixture.store.get_agent(&JsString::from("child")).unwrap().model).unwrap(),
        json!({"providerId":"faux","modelId":"new","thinkingLevel":"high"})
    );
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    fixture.child.prompt("second", PromptOptions::default()).await.unwrap();
    assert_eq!(
        *observed.lock().unwrap(),
        vec![
            ("old".into(), Some(pi_ai::types::ThinkingLevel::Low)),
            ("new".into(), Some(pi_ai::types::ThinkingLevel::High))
        ]
    );
    assert_eq!(
        before_settings,
        pi_js_compat::js_json::to_value(&fixture.settings.lock().unwrap().get_global_settings()).unwrap(),
        "child mutation must not change global defaults"
    );
}

struct BoundaryChild(Option<Arc<dyn pi_coding_agent::core::extensions::types::ViewedSessionMutationTarget>>);
impl pi_coding_agent::bundled_extensions::agents_core::shared::ChildAgentSession for BoundaryChild {
    fn messages(&self) -> Vec<std::sync::Arc<pi_agent_core::agent::types::AgentMessage>> {
        Vec::new()
    }
    fn prompt(&self, _text: JsString) -> pi_ai::request::BoxFuture<'static, Result<(), JsString>> {
        Box::pin(async { panic!("boundary fixture must not prompt") })
    }
    fn transcript(&self) -> Option<pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata> {
        None
    }
    fn session_mutation_target(
        &self,
    ) -> Option<Arc<dyn pi_coding_agent::core::extensions::types::ViewedSessionMutationTarget>> {
        self.0.clone()
    }
}
struct EmptyModelTarget {
    level: Mutex<ThinkingLevel>,
    registry: Arc<pi_coding_agent::core::model_registry::ModelRegistry>,
}
impl pi_coding_agent::core::extensions::types::SessionMutationTarget for EmptyModelTarget {
    fn model(&self) -> Option<pi_ai::types::Model> {
        None
    }
    fn thinking_level(&self) -> ThinkingLevel {
        *self.level.lock().unwrap()
    }
    fn set_model(
        &self,
        _model: pi_ai::types::Model,
    ) -> pi_ai::request::BoxFuture<'static, Result<bool, pi_coding_agent::core::extensions::types::HandlerError>> {
        Box::pin(async { panic!("unexpected model selection") })
    }
    fn set_thinking_level(
        &self,
        level: ThinkingLevel,
    ) -> Result<(), pi_coding_agent::core::extensions::types::HandlerError> {
        *self.level.lock().unwrap() = level;
        Ok(())
    }
}
impl pi_coding_agent::core::extensions::types::ViewedSessionMutationTarget for EmptyModelTarget {
    fn cycle_model(
        &self,
        _direction: pi_coding_agent::core::agent_session::CycleDirection,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<
            Option<pi_coding_agent::core::agent_session::ModelCycleResult>,
            pi_coding_agent::core::extensions::types::HandlerError,
        >,
    > {
        Box::pin(async { panic!("this fixture does not cycle models") })
    }

    fn set_model_with_thinking_level(
        &self,
        _model: pi_ai::types::Model,
        _level: Option<ThinkingLevel>,
    ) -> pi_ai::request::BoxFuture<'static, Result<bool, pi_coding_agent::core::extensions::types::HandlerError>> {
        Box::pin(async { panic!("unexpected model selection") })
    }
    fn model_registry(&self) -> Arc<pi_coding_agent::core::model_registry::ModelRegistry> {
        self.registry.clone()
    }
    fn scoped_models(&self) -> Vec<pi_coding_agent::core::model_resolver::ScopedModel> {
        Vec::new()
    }
}

#[tokio::test]
async fn missing_mutation_surface_and_absent_model_properties_match_source() {
    let fixture = live_fixture().await;
    let mut oracle = oracle::TsOracle::spawn(
        "agent-model-tool-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts",
    );
    let extension = registered(MultiAgentExtensionOptions {
        store: Some(fixture.store.clone()),
        runtime_handles: Some(fixture.handles.clone()),
        ..Default::default()
    })
    .await;
    let definition = extension
        .read()
        .tools
        .get("set_agent_model")
        .unwrap()
        .definition
        .clone();
    let harness = bundled::Harness::new(
        vec![extension],
        create_extension_runtime(),
        bundled::HarnessOptions::new("/fixture"),
    )
    .await;
    let params = json!({"agentId":"child", "thinkingLevel":"medium"});
    fixture
        .handles
        .sessions
        .lock()
        .unwrap()
        .insert("child".into(), Arc::new(BoundaryChild(None)));
    let error = (definition.execute)(
        "fixture".into(),
        params.clone(),
        None,
        None,
        Some(harness.runner.create_context()),
    )
    .await
    .unwrap_err();
    assert_eq!(
        json!({"error":error.message,"selected":"viewed"}),
        oracle.request(
            &json!({"params":params,"agents":fixture.store.list_agents(),"selected":"viewed","live":["child"]})
        )
    );
    fixture.handles.sessions.lock().unwrap().insert(
        "child".into(),
        Arc::new(BoundaryChild(Some(Arc::new(EmptyModelTarget {
            level: Mutex::new(ThinkingLevel::Off),
            registry: Arc::new(fixture.child.model_registry().clone()),
        })))),
    );
    let result = (definition.execute)(
        "fixture".into(),
        params.clone(),
        None,
        None,
        Some(harness.runner.create_context()),
    )
    .await
    .unwrap();
    assert_eq!(
        json!({"result":result,"selected":"viewed"}),
        oracle.request(
            &json!({"op":"emptyModel","params":params,"agents":fixture.store.list_agents(),"selected":"viewed"})
        )
    );
}

#[tokio::test]
async fn source_owned_state_and_fresh_native_state_are_readable_by_same_source_after_tool_mutation() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-model-tool-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts",
    );
    for source_created in [true, false] {
        let fixture = live_fixture_with_persistence(Some((&mut oracle, source_created, "fixture-parent"))).await;
        let path = fixture._dir.path().join("control.db");
        let session = fixture._dir.path().join("parent.session");
        let read = json!({"op":"read","db":path,"session":session,"agentId":"child"});
        let mut manifest = oracle.request(&json!({"op":"manifest","db":path}));
        assert_eq!(manifest["userVersion"], json!(15));
        manifest["direction"] = json!(if source_created {
            "TS-create/Rust-mutate/same-TS-read"
        } else {
            "fresh-Rust-create/Rust-mutate/TS-read"
        });
        manifest["initial"] = oracle.request(&read);
        manifest["observations"] = json!([]);
        assert_eq!(
            oracle.request(&read),
            pi_js_compat::js_json::to_value(&fixture.store.get_agent(&JsString::from("child")).unwrap()).unwrap()
        );
        let extension = registered(MultiAgentExtensionOptions {
            store: Some(fixture.store.clone()),
            runtime_handles: Some(fixture.handles.clone()),
            ..Default::default()
        })
        .await;
        let definition = extension
            .read()
            .tools
            .get("set_agent_model")
            .unwrap()
            .definition
            .clone();
        let harness = bundled::Harness::new(
            vec![extension],
            create_extension_runtime(),
            bundled::HarnessOptions::new("/fixture"),
        )
        .await;
        for params in [
            json!({"agentId":"child","provider":"faux","modelId":"plain","thinkingLevel":"ultra"}),
            json!({"agentId":"child","thinkingLevel":"high"}),
        ] {
            let result = (definition.execute)(
                "fixture".into(),
                params,
                None,
                None,
                Some(harness.runner.create_context()),
            )
            .await
            .unwrap();
            assert_eq!(
                result.details.as_ref().unwrap()["thinkingLevel"],
                pi_js_compat::js_json!("off")
            );
            let source_read = oracle.request(&read);
            assert_eq!(
                source_read,
                pi_js_compat::js_json::to_value(&fixture.store.get_agent(&JsString::from("child")).unwrap()).unwrap()
            );
            assert_eq!(
                source_read["model"],
                json!({"providerId":"faux","modelId":"plain","thinkingLevel":"off"})
            );
            assert_eq!(
                source_read["revision"],
                json!(1),
                "model metadata must preserve lifecycle revision"
            );
            assert_eq!(
                fixture.store.get_selected_agent_id().clone(),
                Some(JsString::from("viewed"))
            );
            manifest["observations"].as_array_mut().unwrap().push(source_read);
        }
        use sha2::Digest;
        manifest["initialSha256"] = json!(format!(
            "{:x}",
            sha2::Sha256::digest(serde_json::to_vec(&manifest["initial"]).unwrap())
        ));
        let proof = std::path::PathBuf::from(std::env::var("PI_CODING_AGENT_DIR").unwrap()).join(format!(
            "agent-model-manifest-{}.json",
            if source_created { "source" } else { "native" }
        ));
        std::fs::write(&proof, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        println!("fixture manifest: {}", proof.display());
    }
}

#[tokio::test]
async fn mismatched_persisted_owner_rejects_model_and_effort_before_live_mutation() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-model-tool-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts",
    );
    let fixture = live_fixture_with_persistence(Some((&mut oracle, true, "different-owner"))).await;
    let read = json!({"op":"read","db":fixture._dir.path().join("control.db"),"session":fixture._dir.path().join("parent.session"),"agentId":"child"});
    let before = oracle.request(&read);
    let extension = registered(MultiAgentExtensionOptions {
        store: Some(fixture.store.clone()),
        runtime_handles: Some(fixture.handles.clone()),
        ..Default::default()
    })
    .await;
    let definition = extension
        .read()
        .tools
        .get("set_agent_model")
        .unwrap()
        .definition
        .clone();
    let harness = bundled::Harness::new(
        vec![extension],
        create_extension_runtime(),
        bundled::HarnessOptions::new("/fixture"),
    )
    .await;
    for params in [
        json!({"agentId":"child","provider":"faux","modelId":"new","thinkingLevel":"medium"}),
        json!({"agentId":"child","thinkingLevel":"high"}),
    ] {
        let error = (definition.execute)(
            "fixture".into(),
            params,
            None,
            None,
            Some(harness.runner.create_context()),
        )
        .await
        .unwrap_err();
        assert_eq!(error.message, "Persisted agent child metadata update was rejected");
        assert_eq!(fixture.child.model().id, "old");
        assert_eq!(fixture.child.thinking_level(), ThinkingLevel::Low);
        assert_eq!(oracle.request(&read), before);
    }
}

#[tokio::test]
async fn schema_argument_admission_matches_source_validation() {
    let extension = registered(MultiAgentExtensionOptions::default()).await;
    let definition = extension
        .read()
        .tools
        .get("set_agent_model")
        .unwrap()
        .definition
        .clone();
    let mut oracle = oracle::TsOracle::spawn(
        "agent-model-tool-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts",
    );
    let mut cases = vec![
        json!({}),
        json!({"agentId":"child", "extra":true}),
        json!({"agentId":"child", "thinkingLevel":"invalid"}),
        json!({"agentId":1, "provider":null, "modelId":null}),
        json!({"agentId":"child", "thinkingLevel":null}),
        json!({"agentId":"child", "provider":false, "modelId":7}),
    ];
    for level in ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"] {
        cases.push(json!({"agentId":"child", "thinkingLevel":level}));
    }
    let expected = oracle.request(&json!({"op":"validate", "cases":cases}));
    let tool = pi_ai::types::Tool {
        name: definition.name.clone(),
        description: definition.description,
        parameters: definition.parameters,
        label: None,
        execution_mode: None,
        typebox_kind: true,
    };
    let actual: Vec<_> = cases
        .iter()
        .map(|params| {
            let call = pi_ai::types::ToolCall::new("fixture", "set_agent_model", params.as_object().unwrap().clone());
            match pi_ai::utils::validation::validate_tool_arguments(&tool, &call) {
                Ok(args) => json!({"arguments":args}),
                Err(error) => json!({"error":error.to_string()}),
            }
        })
        .collect();
    assert_eq!(json!(actual), expected);
}

#[tokio::test]
async fn parent_default_set_model_path_retains_model_and_effort_defaults() {
    let fixture = live_fixture().await;
    let cwd = fixture._dir.path().display().to_string();
    let parent = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone().into()),
        agent_dir: Some(cwd.clone().into()),
        model_registry: Some(fixture.child.model_registry().clone()),
        model: fixture.faux.get_model(Some("old")),
        thinking_level: Some(ThinkingLevel::Low),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(SessionManager::in_memory(Some(&JsString::from(&cwd)), None).unwrap()),
        settings_manager: Some(fixture.settings.clone()),
        control_db_path: Some(
            fixture
                ._dir
                .path()
                .join("parent-control.db")
                .display()
                .to_string()
                .into(),
        ),
        ..Default::default()
    })
    .await
    .unwrap()
    .session;
    parent.set_thinking_level(ThinkingLevel::High).unwrap();
    parent
        .set_model(fixture.faux.get_model(Some("plain")).unwrap())
        .await
        .unwrap();
    assert_eq!(parent.model().id, "plain");
    assert_eq!(parent.thinking_level(), ThinkingLevel::Off);
    let settings = fixture.settings.lock().unwrap();
    assert_eq!(settings.get_default_provider(), Some(&JsString::from("faux")));
    assert_eq!(settings.get_default_model(), Some(&JsString::from("plain")));
    assert_eq!(settings.get_default_thinking_level(), Some(ThinkingLevel::High));
}

#[tokio::test]
async fn live_effective_results_and_effort_literals_match_frozen_session() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-model-tool-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts",
    );
    let mut cases = vec![
        json!({"provider":"faux", "modelId":"new"}),
        json!({"provider":"faux", "modelId":"plain", "thinkingLevel":"high"}),
        json!({"provider":"faux", "modelId":"unknown"}),
        json!({"provider":"faux", "modelId":"new", "thinkingLevel":"medium"}),
        json!({"provider":"  ", "modelId":"\u{feff}", "thinkingLevel":"low"}),
    ];
    for level in ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"] {
        cases.push(json!({"thinkingLevel":level}));
    }
    for mut params in cases {
        params["agentId"] = json!("child");
        let fixture = live_fixture().await;
        let extension = registered(MultiAgentExtensionOptions {
            store: Some(fixture.store.clone()),
            runtime_handles: Some(fixture.handles.clone()),
            ..Default::default()
        })
        .await;
        let definition = extension
            .read()
            .tools
            .get("set_agent_model")
            .unwrap()
            .definition
            .clone();
        let harness = bundled::Harness::new(
            vec![extension],
            create_extension_runtime(),
            bundled::HarnessOptions::new("/fixture"),
        )
        .await;
        let expected = oracle.request(&json!({"op":"success", "cwd":fixture._dir.path(), "params":params, "agents":fixture.store.list_agents(), "selected":"viewed"}));
        let actual = match (definition.execute)(
            "fixture".into(),
            params.clone(),
            None,
            None,
            Some(harness.runner.create_context()),
        )
        .await
        {
            Ok(result) => {
                json!({"result":result,"selected":fixture.store.get_selected_agent_id(), "model":fixture.store.get_agent(&JsString::from("child")).unwrap().model})
            }
            Err(error) => json!({"error":error.message,"selected":fixture.store.get_selected_agent_id()}),
        };
        assert_eq!(actual, expected, "{params:?}");
    }
}

#[tokio::test]
async fn registers_agent_model_tool_without_approval() {
    let extension = registered(MultiAgentExtensionOptions::default()).await;
    let extension = extension.read();
    let tool = &extension
        .tools
        .get("set_agent_model")
        .expect("set_agent_model registration")
        .definition;
    assert_eq!(tool.label, "Set Agent Model");
    assert_eq!(tool.approval_required, Some(false));
    assert_eq!(tool.parameters["additionalProperties"], false);
    assert_eq!(
        pi_js_compat::js_json::JsValue::from(tool.parameters["required"].clone()),
        pi_js_compat::js_json!(["agentId"])
    );
    let mut oracle = oracle::TsOracle::spawn(
        "agent-model-tool-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts",
    );
    let actual = pi_js_compat::js_json!({
        "name": tool.name, "label": tool.label, "description": tool.description,
        "promptGuidelines": tool.prompt_guidelines, "approvalRequired": tool.approval_required,
        "parameters": tool.parameters,
    });
    assert_eq!(actual, oracle.request(&pi_js_compat::js_json!({"op": "metadata"})));
}

use crate::shared_scripted_registry as scripted_registry;

// Shared support is compiled once per integration target.
#[path = "support/bundled.rs"]
pub mod shared_bundled;
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
