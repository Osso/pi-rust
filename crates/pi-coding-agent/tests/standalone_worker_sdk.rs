use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};

#[tokio::test]
async fn standalone_sdk_defaults_to_an_in_memory_session() {
    let temp = tempfile::TempDir::new().unwrap();
    let result = create_agent_session(CreateAgentSessionOptions {
        no_supervisor: Some(true),
        cwd: Some(temp.path().to_str().unwrap().into()),
        agent_dir: Some(temp.path().to_str().unwrap().into()),
        model_registry: Some(ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new()))),
        resource_loader: Some(StaticResourceLoader::new()),
        ..Default::default()
    })
    .await
    .unwrap();
    let persisted = result.session.session_manager().is_persisted();
    result.session.dispose();
    assert!(!persisted, "standalone worker must never create a persisted session");
}
