use crate::shared_oracle as oracle;

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
        model_registry: Some(ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap()),
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
    let (_, rebuilt_tokens) = session.session_manager().build_session_context_with_identities();
    assert_eq!(
        rebuilt_tokens.last(),
        after_bash.message_tokens().last(),
        "bash persistence shares the Agent's message object"
    );
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

#[test]
fn context_rebuild_preserves_stored_message_identity_but_recreates_synthetic_messages() {
    use pi_agent_core::agent::AgentMessageIdentity;
    use pi_coding_agent::core::session_manager::SessionManager;
    let mut manager = SessionManager::in_memory(Some(&"/tmp".into()), None).unwrap();
    let message = AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text("same".into()),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    });
    let first = manager.append_message(message.clone()).unwrap();
    let token = AgentMessageIdentity::new();
    manager.bind_message_identity(&first, token.clone()).unwrap();
    let second = manager.append_message(message).unwrap();
    manager
        .append_custom_message_entry(
            &"notice".into(),
            CustomMessageContent::Text("synthetic".into()),
            true,
            None,
        )
        .unwrap();
    let (left, left_tokens) = manager.build_session_context_with_identities();
    let (right, right_tokens) = manager.build_session_context_with_identities();
    assert_eq!(left, right);
    assert_eq!(left_tokens[0], token);
    assert_eq!(&left_tokens[..2], &right_tokens[..2]);
    assert_ne!(
        left_tokens[0], left_tokens[1],
        "equal values are distinct stored references"
    );
    assert_ne!(
        left_tokens[2], right_tokens[2],
        "custom projection creates a new object per rebuild"
    );
    assert_eq!(manager.message_entry_id(&left_tokens[0]), Some(&first));
    assert_eq!(manager.message_entry_id(&left_tokens[1]), Some(&second));
    assert_eq!(manager.message_entry_id(&left_tokens[2]), None);
    let mut source = oracle::TsOracle::spawn(
        "session-context-identity-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"id":1}));
    assert_eq!(
        expected["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{expected:?}"
    );
    assert_eq!(
        expected["output"],
        pi_js_compat::js_json!({
            "storedSame":[left_tokens[0]==right_tokens[0],left_tokens[1]==right_tokens[1]],
            "equalValuesDistinct":left_tokens[0]!=left_tokens[1],
            "syntheticSame":left_tokens[2]==right_tokens[2],
            "firstIsInput":left_tokens[0]==token,
            "secondIsInput":left_tokens[1]==token,
        })
    );
}
