use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::messages::CustomMessageContent;
use pi_ai::{UserContent, UserMessage, UserRole};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};

#[tokio::test]
async fn custom_and_bash_appends_preserve_existing_message_identity() {
    let temp = tempfile::TempDir::new().unwrap();
    let session = create_agent_session(CreateAgentSessionOptions {
        no_supervisor: Some(true),
        cwd: Some(temp.path().to_str().unwrap().into()),
        agent_dir: Some(temp.path().to_str().unwrap().into()),
        model_registry: Some(ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new()))),
        resource_loader: Some(StaticResourceLoader::new()),
        ..Default::default()
    })
    .await
    .unwrap()
    .session;
    session.agent().set_messages(vec![AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text("original".into()),
        input_source: None,
        provider_native: None,
        timestamp: 0.0,
    })]);
    let original = session.agent().identity_snapshot(|_| true);
    session
        .send_custom_message(
            &"notice".into(),
            CustomMessageContent::Text("append".into()),
            true,
            None,
            false,
            None,
        )
        .await
        .unwrap();
    let after_custom = session.agent().identity_snapshot(|_| true);
    assert_eq!(after_custom.message_tokens().len(), 2);
    assert_eq!(original.message_tokens(), &after_custom.message_tokens()[..1]);
    session
        .execute_bash(&"printf identity".into(), None, Default::default())
        .await
        .unwrap();
    let after_bash = session.agent().identity_snapshot(|_| true);
    assert_eq!(after_bash.message_tokens().len(), 3);
    assert_eq!(after_custom.message_tokens(), &after_bash.message_tokens()[..2]);
    session.agent().truncate_messages(1);
    let truncated = session.agent().identity_snapshot(|_| true);
    assert!(original.has_same_model(&truncated));
    assert_eq!(original.message_tokens(), truncated.message_tokens());
    session.agent().set_messages(session.messages());
    let replaced = session.agent().identity_snapshot(|_| true);
    assert_ne!(original.message_tokens(), replaced.message_tokens());
    session.dispose();
}
