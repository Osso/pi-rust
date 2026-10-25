#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::{
    core::{
        auth_storage::AuthStorage, model_registry::ModelRegistry, resource_loader::StaticResourceLoader,
        session_manager::SessionManager,
    },
    supervisor::main::{CreateSupervisorSessionOptions, create_supervisor_session, create_supervisor_settings_manager},
};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::sync::{Arc, Mutex};
const MODULE: &str = "packages/coding-agent/src/supervisor/main.ts";

#[tokio::test]
async fn authenticated_supervisor_model_selection_matches_frozen_requirements() {
    for (providers, selected) in [
        (vec![], None),
        (vec!["openai-codex-gc"], Some("openai-codex-gc")),
        (
            vec!["openai-codex-team", "openai-codex-gc", "openai-codex"],
            Some("openai-codex"),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let agent_dir = JsString::from(directory.path().to_str().unwrap());
        let mut records = pi_js_compat::js_json::JsObject::new();
        for provider in providers {
            records.insert(provider.into(), json!({"type":"api_key","key":"fixture"}));
        }
        let auth_path = directory.path().join("auth.json");
        std::fs::write(
            &auth_path,
            pi_js_compat::js_json::stringify_lossless(&JsValue::Object(records)),
        )
        .unwrap();
        let mut source = oracle::TsOracle::spawn("coding-agent-supervisor-main-oracle.ts", MODULE);
        let expected = source.request(&json!({"id":1,"operation":"modelRequirements","agentDir":agent_dir}));
        assert_eq!(expected["ok"], json!(true));
        assert_eq!(expected["output"]["available"], json!(selected.is_some()));
        let registry = ModelRegistry::create(
            AuthStorage::create(Some(&auth_path.to_str().unwrap().into())).unwrap(),
            Some(directory.path().join("models.json").to_str().unwrap().into()),
        )
        .unwrap();
        let result = create_supervisor_session(CreateSupervisorSessionOptions {
            agent_dir: agent_dir.clone(),
            kb_dir: agent_dir.clone(),
            model_registry: registry,
            resource_loader: StaticResourceLoader::new(),
            session_manager: SessionManager::in_memory(Some(&agent_dir), None).unwrap(),
            settings_manager: Arc::new(Mutex::new(create_supervisor_settings_manager())),
        })
        .await;
        match selected {
            Some(provider) => {
                let session = result.unwrap().session;
                assert_eq!(session.model().provider, provider);
                assert_eq!(session.model().id, "gpt-6.1-sol");
                assert_eq!(expected["output"]["error"], JsValue::Null);
            }
            None => assert_eq!(
                result.err().expect("missing authentication must reject"),
                expected["output"]["error"].as_string().unwrap().clone()
            ),
        }
    }
}
