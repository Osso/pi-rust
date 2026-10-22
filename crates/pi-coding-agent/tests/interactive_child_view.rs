//! Behavioral tests for the interactive selected-agent transcript boundary.

use pi_agent_core::agent::types::AgentMessage;
use pi_ai::providers::faux::{FauxAssistantMessageOptions, faux_assistant_message};
use pi_ai::types::{UserContent, UserMessage, UserRole};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::modes::interactive::interactive_mode::read_child_transcript;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

fn user(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(JsString::from(text.to_owned())),
        input_source: None,
        provider_native: None,
        timestamp: 0.0,
    })
}

#[test]
fn read_child_transcript_returns_the_child_messages() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let session_dir = dir.path().join("sessions").display().to_string();
    let mut manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
        .expect("session manager");
    manager.append_message(user("child task")).expect("user message");
    manager
        .append_message(AgentMessage::Assistant(Box::new(faux_assistant_message(
            "child done",
            FauxAssistantMessageOptions::default(),
        ))))
        .expect("assistant message");
    manager.persist_for_recovery().expect("persist");
    let path = manager.get_session_file().expect("session file").to_owned();

    let messages = read_child_transcript(&path, &JsString::from(&session_dir)).expect("transcript");
    assert_eq!(messages.len(), 2);
    assert!(
        matches!(&messages[0], AgentMessage::User(message) if matches!(&message.content, UserContent::Text(text) if text == "child task"))
    );
    assert!(
        matches!(&messages[1], AgentMessage::Assistant(message) if message.content.iter().any(|block| matches!(block, pi_ai::types::AssistantContent::Text(text) if text.text == "child done")))
    );

    assert!(read_child_transcript(&js_format!("{}.missing", path), &JsString::from(&session_dir)).is_none());
}
