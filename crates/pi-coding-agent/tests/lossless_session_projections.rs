//! Reopened session records stay raw (`SessionEntry::Unknown`); every consumer must read them
//! through projections and see the same records it sees for entries appended in-process.

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, Usage, UserContent, UserMessage,
    UserRole,
};
use pi_coding_agent::core::compaction::{get_last_assistant_usage, prepare_branch_entries};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::JsString;

fn user_message(text: &str, timestamp: i64) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(JsString::from(text)),
        input_source: None,
        provider_native: None,
        timestamp: timestamp as f64,
    })
}

fn assistant_message(text: &str, timestamp: i64, usage: Usage) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: vec![AssistantContent::Text(TextContent::new(text))],
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-test".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage,
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: timestamp as f64,
    }))
}

#[test]
fn reopened_records_feed_compaction_and_branch_consumers_like_live_records() {
    let directory = tempfile::tempdir().unwrap();
    let root = JsString::from(directory.path().to_str().expect("utf-8 test path"));
    let mut manager = SessionManager::create(&root, Some(&root), None).unwrap();
    manager.append_message(user_message("hello", 1)).unwrap();
    let usage = Usage {
        input: 7.0,
        output: 3.0,
        ..Usage::default()
    };
    manager.append_message(assistant_message("world", 2, usage)).unwrap();
    let file = manager.get_session_file().unwrap().to_owned();
    let live = manager.get_entries();
    let reopened = SessionManager::open(&file, None, None).unwrap().get_entries();

    let live_usage = get_last_assistant_usage(&live).expect("live assistant usage");
    assert_eq!(live_usage.input, 7.0);
    assert_eq!(get_last_assistant_usage(&reopened), Some(live_usage));

    let live_branch = prepare_branch_entries(&live, 100_000.0);
    assert_eq!(live_branch.messages.len(), 2);
    assert_eq!(prepare_branch_entries(&reopened, 100_000.0), live_branch);
}
