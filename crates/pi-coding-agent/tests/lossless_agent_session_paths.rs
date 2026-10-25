#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::{Agent, AgentOptions};
use pi_coding_agent::core::agent_session::{AgentSession, AgentSessionConfig};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::session_control_db::RuntimeMailboxStoreRef;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn ts_created_raw_session_path_survives_agent_session_persistence_identity() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let directory = cwd.clone() + "/sessions-" + JsString::from_utf16(vec![0xd800]);
    let mut oracle = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    assert_eq!(oracle.request(&serde_json::json!({"id":"scalar-probe", "operation":"default_session_dir", "cwd":root.path().to_str().unwrap(), "agentDir":root.path().to_str().unwrap()}))["ok"], true);
    let source = oracle.request(&pi_js_compat::js_json!({
        "id":"raw-path-session", "operation":"write_session", "cwd":cwd, "dir":directory,
        "messages":[{"role":"user","content":"seed","timestamp":0}, {"role":"assistant","content":[],"api":"openai-responses","provider":"openai","model":"probe","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":1}]
    }));
    assert_eq!(source["ok"].as_bool(), Some(true));
    let path = source["output"]["path"].as_string().unwrap().clone();
    assert!(path.units().contains(&0xd800));
    let manager = SessionManager::open(&path, Some(&directory), None).unwrap();
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(pi_js_compat::js_json::JsObject::new())).unwrap();
    let model = runtime.get_all().into_iter().next().unwrap();
    assert_eq!(runtime.find(&model.provider, &model.id), Some(model.clone()));
    let invalid_provider = JsString::from(&model.provider) + JsString::from_utf16(vec![0xd800]);
    assert!(runtime.find(invalid_provider, &model.id).is_none());
    let invalid_model = JsString::from(&model.id) + JsString::from_utf16(vec![0xdc00]);
    assert!(runtime.find(&model.provider, invalid_model).is_none());
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    // main.ts:1127-1128 binds the control DB and restores the store before the session exists.
    let mut manager = manager;
    manager
        .set_metadata_control_db_path(Some(&(cwd.clone() + "/control.db")), Default::default())
        .unwrap();
    store.restore_from_session_manager(&manager).unwrap();
    let session = AgentSession::new(AgentSessionConfig {
        agent: Agent::new(AgentOptions::default()).unwrap(),
        session_manager: Arc::new(Mutex::new(manager)),
        settings_manager: Arc::new(Mutex::new(SettingsManager::in_memory(
            Default::default(),
            SettingsManagerCreateOptions::default(),
        ))),
        cwd: cwd.clone(),
        scoped_models: vec![],
        resource_loader: Arc::new(StaticResourceLoader::default()),
        model_registry: runtime.clone(),
        initial_active_tool_names: Some(vec![]),
        allowed_tool_names: Some(vec![]),
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(cwd.clone()),
        control_db_path: Some(cwd.clone() + "/control.db"),
        base_tools: vec![],
        custom_tools: vec![],
        custom_tool_definitions: vec![],
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: None,
        no_supervisor: None,
        supervisor_decision_requester: None,
        multi_agent_store: Some(store.clone()),
        multi_agent_agent_id: None,
        multi_agent_parent_session_id: None,
        multi_agent_requires_agent_id: false,
        disable_runtime_coordination_inbound: true,
        on_shared_channel_message_delivered: None,
        extension_runner_ref: None,
        session_start_event: None,
    })
    .unwrap();
    let missing_id = JsString::from("missing-") + JsString::from_utf16(vec![0xd800]);
    let missing_compaction = session.materialize_compaction_summary(&missing_id).await.unwrap_err();
    assert_eq!(
        missing_compaction.message(),
        &(JsString::from("Entry ") + &missing_id + " not found")
    );
    let missing_tree = session
        .navigate_tree(&missing_id, Default::default())
        .await
        .unwrap_err();
    assert_eq!(missing_tree.message(), missing_compaction.message());
    assert_eq!(session.session_file(), Some(path.clone()));
    let target = store.get_persistence_target().unwrap();
    assert_eq!(target.session_path, path);
    let reference = RuntimeMailboxStoreRef {
        message_id: "message".into(),
        session_path: path.clone(),
    };
    assert_eq!(reference.session_path, target.session_path);
    let os_alias = RuntimeMailboxStoreRef {
        message_id: "message".into(),
        session_path: encode_node_utf8(&path).into(),
    };
    assert_ne!(os_alias.session_path, target.session_path);
    session.dispose();
    let reopened = SessionManager::open(&path, Some(&directory), None).unwrap();
    assert_eq!(reopened.get_session_file(), Some(&path));
    let sdk = pi_coding_agent::core::sdk::create_agent_session(pi_coding_agent::core::sdk::CreateAgentSessionOptions {
        cwd: Some(cwd.clone()),
        agent_dir: Some(directory.clone()),
        model_registry: Some(runtime),
        resource_loader: Some(Arc::new(StaticResourceLoader::default())),
        settings_manager: Some(Arc::new(Mutex::new(SettingsManager::in_memory(
            Default::default(),
            SettingsManagerCreateOptions::default(),
        )))),
        session_manager: Some(reopened),
        no_tools: Some(pi_coding_agent::core::sdk::NoTools::All),
        control_db_path: Some(cwd + "/control.db"),
        disable_runtime_coordination_inbound: true,
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(sdk.session.session_file(), Some(path));
    assert_eq!(sdk.session.agent_dir(), &directory);
    sdk.session.dispose();
}

#[tokio::test]
async fn raw_storage_paths_keep_identity_until_the_filesystem_boundary() {
    use pi_coding_agent::core::auth_storage::FileAuthStorageBackend;
    let root = tempfile::tempdir().unwrap();
    let raw = JsString::from(root.path().to_str().unwrap()) + "/raw-" + JsString::from_utf16(vec![0xd800]);
    std::fs::create_dir_all(encode_node_utf8(&raw)).unwrap();
    let auth_path = raw.clone() + "/auth.json";
    std::fs::write(encode_node_utf8(&auth_path), "{}").unwrap();
    let backend = FileAuthStorageBackend::new(&auth_path).unwrap();
    assert_eq!(backend.auth_path(), &auth_path);
    let models_path = raw.clone() + "/models.json";
    std::fs::write(encode_node_utf8(&models_path), "{").unwrap();
    let config = ModelRegistry::create(
        AuthStorage::in_memory([] as [(&str, serde_json::Value); 0]),
        Some(models_path.clone()),
    )
    .unwrap();
    assert!(
        config
            .get_error()
            .unwrap()
            .ends_with(&(JsString::from("File: ") + &models_path))
    );
    let mut settings = SettingsManager::create(&raw, Some(&raw), Default::default()).unwrap();
    assert!(settings.drain_errors().is_empty());
}

#[tokio::test]
async fn runtime_factory_error_and_cwd_keep_original_units() {
    use pi_coding_agent::core::agent_session_runtime::{
        CreateAgentSessionRuntimeFactory, create_agent_session_runtime,
    };
    let root = tempfile::tempdir().unwrap();
    let raw = JsString::from(root.path().to_str().unwrap()) + "/cwd-" + JsString::from_utf16(vec![0xdc00]);
    std::fs::create_dir_all(encode_node_utf8(&raw)).unwrap();
    let expected_error = JsString::from("factory-") + JsString::from_utf16(vec![0xd800]);
    let seen = Arc::new(Mutex::new(None));
    let factory: CreateAgentSessionRuntimeFactory = Arc::new({
        let seen = seen.clone();
        let error = expected_error.clone();
        move |options| {
            *seen.lock().unwrap() = Some((options.cwd, options.agent_dir));
            let error = error.clone();
            Box::pin(async move { Err(pi_ai::request::CallbackError::from(error)) })
        }
    });
    let manager = SessionManager::in_memory(Some(&raw), None).unwrap();
    let result = create_agent_session_runtime(factory, &raw, &raw, manager, None, None).await;
    assert_eq!(result.err().map(|error| error.message), Some(expected_error));
    assert_eq!(*seen.lock().unwrap(), Some((raw.clone(), raw)));
}

#[test]
fn session_resource_cleanup_receives_raw_session_id() {
    let raw = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let seen = Arc::new(Mutex::new(None));
    let captured = seen.clone();
    let registration = pi_ai::session_resources::register_session_resource_cleanup(Box::new(move |id| {
        *captured.lock().unwrap() = id.cloned();
        Ok(())
    }));
    pi_ai::session_resources::cleanup_session_resources(Some(&raw)).unwrap();
    registration.unregister();
    assert_eq!(*seen.lock().unwrap(), Some(raw));
}

#[test]
fn agent_message_event_serialization_retains_raw_text() {
    let raw = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let event = pi_coding_agent::core::agent_session::AgentMessageEvent {
        event: pi_agent_core::agent::types::AgentEvent::MessageEnd {
            message: pi_agent_core::AgentMessage::User(pi_ai::types::UserMessage {
                role: pi_ai::types::UserRole::Tag,
                content: pi_ai::types::UserContent::Text(raw.clone()),
                input_source: None,
                provider_native: None,
                timestamp: 1.0,
            }),
        },
        runtime_message_marker: None,
    };
    let serialized = pi_js_compat::js_json::to_value(&event).unwrap();
    assert_eq!(serialized["message"]["content"].as_string(), Some(&raw));
}

#[test]
fn model_reference_matching_does_not_replace_unmatched_units() {
    use pi_coding_agent::core::model_resolver::find_exact_model_reference_match;
    let model: pi_ai::types::Model = serde_json::from_value(serde_json::json!({
        "id":"probe", "name":"probe", "api":"openai-responses", "provider":"vendor�", "baseUrl":"https://provider.invalid/v1", "reasoning":false, "input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}, "contextWindow":10000, "maxTokens":1000
    })).unwrap();
    let raw = JsString::from("vendor") + JsString::from_utf16(vec![0xd800]) + "/probe";
    let mut oracle = oracle::TsOracle::spawn(
        "model-resolver-oracle.ts",
        "packages/coding-agent/src/core/model-resolver.ts",
    );
    let source = oracle.request(
        &pi_js_compat::js_json!({"id":"raw-model-reference", "operation":"exact", "reference":raw, "models":[model]}),
    );
    assert_eq!(source["ok"].as_bool(), Some(true));
    assert!(source["output"]["model"].is_null());
    assert!(find_exact_model_reference_match(&raw, std::slice::from_ref(&model)).is_none());
    assert_eq!(
        find_exact_model_reference_match(&"vendor�/probe".into(), std::slice::from_ref(&model)),
        Some(model)
    );
}

#[tokio::test]
async fn invalid_skill_frontmatter_emits_native_path_error_and_keeps_original_command() {
    use pi_coding_agent::core::resource_loader::LoadedSkills;
    use pi_coding_agent::core::skills::Skill;
    use pi_coding_agent::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let path = cwd.clone() + "/skill-" + JsString::from_utf16(vec![0xd800]) + ".md";
    std::fs::write(encode_node_utf8(&path), "---\ndescription: [broken\n---\nbody").unwrap();
    let skill = Skill {
        name: "broken".into(),
        description: "injected skill".into(),
        file_path: path.clone(),
        base_dir: cwd.clone(),
        source_info: create_synthetic_source_info(
            &path,
            &SyntheticSourceInfoOptions {
                source: "local".into(),
                scope: None,
                origin: None,
                base_dir: Some(cwd.clone()),
            },
        ),
        disable_model_invocation: false,
    };
    let loader = StaticResourceLoader {
        skills: LoadedSkills {
            skills: vec![skill],
            diagnostics: vec![],
        },
        ..Default::default()
    };
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(pi_js_compat::js_json::JsObject::new())).unwrap();
    let created =
        pi_coding_agent::core::sdk::create_agent_session(pi_coding_agent::core::sdk::CreateAgentSessionOptions {
            cwd: Some(cwd.clone()),
            agent_dir: Some(cwd.clone()),
            model_registry: Some(runtime),
            resource_loader: Some(Arc::new(loader)),
            settings_manager: Some(Arc::new(Mutex::new(SettingsManager::in_memory(
                Default::default(),
                Default::default(),
            )))),
            session_manager: Some(SessionManager::in_memory(Some(&cwd), None).unwrap()),
            no_tools: Some(pi_coding_agent::core::sdk::NoTools::All),
            control_db_path: Some(cwd + "/control.db"),
            disable_runtime_coordination_inbound: true,
            ..Default::default()
        })
        .await
        .unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let _subscription = created
        .session
        .extension_runner()
        .on_error(Arc::new(move |error| captured.lock().unwrap().push(error.clone())));
    created.session.follow_up("/skill:broken extra", &[]).await.unwrap();
    assert_eq!(
        created.session.get_follow_up_messages(),
        vec![JsString::from("/skill:broken extra")]
    );
    let errors = seen.lock().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].extension_path, path);
    assert_eq!(errors[0].event, "skill_expansion");
    created.session.dispose();
}
