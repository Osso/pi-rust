//! Frozen agent-model-tool registration and live-child mutation contract.
#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, FauxResponseStep,
    RegisterFauxProviderOptions, faux_assistant_message, faux_provider,
};
use pi_coding_agent::bundled_extensions::agents_core::session_factories::AgentSessionChildSession;
use pi_coding_agent::core::agent_session::{AgentSession, PromptOptions};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::multi_agent_store::{AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use serde_json::json;
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
        "/fixture",
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        "<first-party:agents-core>",
    )
    .await
    .expect("agents-core factory")
}

fn agent(id: &str, lifecycle: &str, agent_type: &str) -> AgentSnapshot {
    serde_json::from_value(json!({
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
    store.select_agent_view("active");
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
            harness.runner.create_context(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            json!({"error": error.message, "selected": store.get_selected_agent_id()}),
            expected,
            "{params}"
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
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(
            serde_json::from_value(json!({"faux": {"type":"api_key", "key":"fixture"}})).unwrap(),
        ))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .unwrap();
    runtime.register_native_provider(faux.provider.clone());
    runtime.refresh(Default::default()).await.unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    for snapshot in [
        agent("child", "running", "explore"),
        agent("viewed", "running", "explore"),
    ] {
        store.publish_lifecycle_coordinator_snapshot(&snapshot).unwrap();
    }
    store.select_agent_view("viewed");
    let settings = Arc::new(Mutex::new(SettingsManager::in_memory(
        Default::default(),
        SettingsManagerCreateOptions::default(),
    )));
    let child = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone()),
        agent_dir: Some(cwd.clone()),
        model_runtime: Some(runtime),
        model: faux.get_model(Some("old")),
        thinking_level: Some(ThinkingLevel::Low),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(SessionManager::in_memory(Some(&cwd), None).unwrap()),
        settings_manager: Some(settings.clone()),
        control_db_path: Some(dir.path().join("control.db").display().to_string()),
        multi_agent_store: Some(store.clone()),
        multi_agent_agent_id: Some("child".into()),
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

#[tokio::test]
async fn live_request_keeps_old_model_next_request_uses_effective_model_and_effort() {
    let fixture = live_fixture().await;
    let before_settings = serde_json::to_value(fixture.settings.lock().unwrap().get_global_settings()).unwrap();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let observed = Arc::new(Mutex::new(Vec::new()));
    let response = |hold: bool| {
        let started = started.clone();
        let release = release.clone();
        let observed = observed.clone();
        FauxResponseStep::Factory(Arc::new(move |_context, options, _state, model| {
            let started = started.clone();
            let release = release.clone();
            let observed = observed.clone();
            Box::pin(async move {
                let options = options.expect("request options");
                observed.lock().unwrap().push((model.id, options.reasoning));
                if hold {
                    started.notify_one();
                    release.notified().await;
                }
                assert!(
                    !options.stream.request.signal.as_ref().unwrap().is_cancelled(),
                    "setter cancelled held request"
                );
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
    let result = (definition.execute)(
        "change".into(),
        json!({"agentId":"child", "provider":" \u{feff}faux\u{a0}", "modelId":" new ", "thinkingLevel":"ultra"}),
        None,
        None,
        harness.runner.create_context(),
    )
    .await
    .unwrap();
    assert_eq!(
        result.details,
        Some(json!({"agentId":"child","provider":"faux","modelId":"new","thinkingLevel":"high"}))
    );
    assert!(!pending.is_finished(), "held request must remain in flight");
    assert_eq!(
        *observed.lock().unwrap(),
        vec![("old".into(), Some(pi_ai::types::ThinkingLevel::Low))]
    );
    assert_eq!(fixture.store.get_selected_agent_id().as_deref(), Some("viewed"));
    assert_eq!(
        serde_json::to_value(fixture.store.get_agent("child").unwrap().model).unwrap(),
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
        serde_json::to_value(fixture.settings.lock().unwrap().get_global_settings()).unwrap(),
        "child mutation must not change global defaults"
    );
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
    assert_eq!(tool.parameters["required"], serde_json::json!(["agentId"]));
    let mut oracle = oracle::TsOracle::spawn(
        "agent-model-tool-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts",
    );
    let actual = serde_json::json!({
        "name": tool.name, "label": tool.label, "description": tool.description,
        "promptGuidelines": tool.prompt_guidelines, "approvalRequired": tool.approval_required,
        "parameters": tool.parameters,
    });
    assert_eq!(actual, oracle.request(&serde_json::json!({"op": "metadata"})));
}
