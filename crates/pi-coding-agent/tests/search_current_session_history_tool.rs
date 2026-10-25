//! Port of `packages/coding-agent/test/search-current-session-history-tool.test.ts`.

use crate::shared_tool_context as tool_context;

use std::sync::{Arc, Mutex};

use pi_agent_core::AgentMessage;
use pi_ai::types::{AssistantContent, AssistantMessage, TextContent};
use pi_coding_agent::core::extensions::types::ContextTool;
use pi_coding_agent::core::session_manager::{CompactionAppendOptions, SessionManager, SharedSessionManager};
use pi_coding_agent::core::tools::search_current_session_history::SearchCurrentSessionHistoryTool;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_js_compat::{js_json, js_json as json};
use tempfile::TempDir;

fn manager_fixture() -> (TempDir, SessionManager) {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let manager = SessionManager::create(
        &JsString::from("/repo"),
        Some(&JsString::from(
            &temp_dir
                .path()
                .join("sessions")
                .to_str()
                .expect("utf-8 test path")
                .to_owned(),
        )),
        None,
    )
    .expect("session manager");
    (temp_dir, manager)
}

fn user_message(text: &str, timestamp: i64) -> AgentMessage {
    js_json::from_value(&json!({ "role": "user", "content": text, "timestamp": timestamp })).expect("user message")
}

fn assistant_message(text: &str, timestamp: i64) -> AgentMessage {
    let mut message = AssistantMessage::empty("openai-responses", "openai", "gpt-5.5", timestamp as f64);
    message.content.push(AssistantContent::Text(TextContent::new(text)));
    AgentMessage::Assistant(Box::new(message))
}

async fn search(manager: &SharedSessionManager, params: Value) -> pi_agent_core::AgentToolResult {
    let context = tool_context::TestContextBuilder::new("/repo", Arc::clone(manager))
        .build()
        .await
        .1;
    SearchCurrentSessionHistoryTool::new()
        .execute(
            "search-history".to_owned(),
            json!(params),
            None,
            None,
            Some(context),
            None,
        )
        .await
        .expect("search history")
}

fn details(result: &pi_agent_core::AgentToolResult) -> &Value {
    result.details.as_ref().expect("details")
}

#[tokio::test]
async fn matches_user_and_assistant_messages() {
    let (_temp_dir, mut manager) = manager_fixture();
    manager
        .append_message(user_message("list-skills", 1))
        .expect("append user");
    manager
        .append_message(assistant_message(
            "Available skills include reddit and /tmp/pi-clipboard-image.png",
            2,
        ))
        .expect("append assistant");
    let manager = Arc::new(Mutex::new(manager));

    let user_match = search(&manager, json!({ "query": "list-skills" })).await;
    let assistant_match = search(&manager, json!({ "query": "reddit" })).await;

    assert_eq!(details(&user_match)["totalMatches"], json!(1));
    assert_eq!(details(&user_match)["entries"][0]["role"], json!("user"));
    assert_eq!(details(&user_match)["entries"][0]["content"], json!("list-skills"));
    assert_eq!(details(&user_match)["entries"][0]["matched"], json!(true));
    assert_eq!(details(&assistant_match)["totalMatches"], json!(1));
    assert_eq!(details(&assistant_match)["entries"][0]["role"], json!("assistant"));
}

#[tokio::test]
async fn searches_full_active_branch_entries_hidden_by_compaction_and_includes_neighbors() {
    let (_temp_dir, mut manager) = manager_fixture();
    let first_id = manager
        .append_message(user_message("Original deployment used cobalt", 1))
        .expect("append first");
    let kept_id = manager
        .append_message(user_message("Neighbor before compaction", 2))
        .expect("append kept");
    manager
        .append_compaction(
            &JsString::from("Earlier deployment discussion"),
            &kept_id,
            1000.0,
            CompactionAppendOptions::default(),
        )
        .expect("append compaction");
    manager
        .append_message(user_message("Current follow-up", 3))
        .expect("append follow-up");
    let manager = Arc::new(Mutex::new(manager));

    let result = search(&manager, json!({ "query": "COBALT", "context_entries": 1 })).await;

    assert_eq!(details(&result)["totalMatches"], json!(1));
    let entries = details(&result)["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["id"], json!(first_id));
    assert_eq!(entries[0]["matched"], json!(true));
    assert_eq!(entries[0]["compacted"], json!(true));
    assert_eq!(entries[0]["role"], json!("user"));
    assert_eq!(entries[1]["matched"], json!(false));
    assert_eq!(entries[1]["content"], json!("Neighbor before compaction"));
    assert_eq!(entries[1]["role"], json!("user"));
    let content = js_json::to_value(&&result.content).expect("content");
    assert!(
        content[0]["text"]
            .as_string()
            .expect("text")
            .contains("Original deployment used cobalt")
    );
}

#[tokio::test]
async fn context_tool_arguments_use_javascript_json_in_visible_text() {
    let (_temp_dir, mut manager) = manager_fixture();
    manager.append_message(user_message("needle", 1)).unwrap();
    let mut message = AssistantMessage::empty("openai-responses", "openai", "gpt", 2.0);
    message
        .content
        .push(AssistantContent::ToolCall(pi_ai::types::ToolCall::new(
            "call",
            "unknown",
            serde_json::from_str(r#"{"2":2.0,"1":1.0}"#).unwrap(),
        )));
    manager
        .append_message(AgentMessage::Assistant(Box::new(message)))
        .unwrap();
    let result = search(
        &Arc::new(Mutex::new(manager)),
        json!({"query":"needle", "context_entries":1}),
    )
    .await;
    let text = js_json::to_value(&result.content).unwrap();
    let text = text[0]["text"].as_string().unwrap();
    assert!(text.contains("\"1\": 1,"), "{text:?}");
    assert!(
        text.find("\"1\": 1,").unwrap() < text.find("\"2\": 2").unwrap(),
        "{text:?}"
    );
    assert!(!text.contains("2.0"), "{text:?}");
}

#[tokio::test]
async fn excludes_entries_from_inactive_branches() {
    let (_temp_dir, mut manager) = manager_fixture();
    let root_id = manager
        .append_message(user_message("shared root", 1))
        .expect("append root");
    manager
        .append_message(user_message("inactive branch secret", 2))
        .expect("append inactive");
    manager.branch(&root_id).expect("branch");
    manager
        .append_message(user_message("active branch", 3))
        .expect("append active");
    let manager = Arc::new(Mutex::new(manager));

    let result = search(&manager, json!({ "query": "inactive branch secret" })).await;

    assert_eq!(details(&result)["totalMatches"], json!(0));
    assert_eq!(details(&result)["entries"], json!([]));
    assert_eq!(
        js_json::to_value(&&result.content).expect("content"),
        json!([{ "type": "text", "text": "No matches found in current session history." }])
    );
}

#[tokio::test]
async fn matches_literal_multiline_and_escaped_characters_in_message_content() {
    let (_temp_dir, mut manager) = manager_fixture();
    let content = "alpha\nbeta says \"quoted\" at C:\\tmp";
    manager
        .append_message(user_message(content, 1))
        .expect("append message");
    let manager = Arc::new(Mutex::new(manager));

    let result = search(&manager, json!({ "query": content })).await;

    assert_eq!(details(&result)["totalMatches"], json!(1));
    assert_eq!(details(&result)["entries"][0]["content"], json!(content));
    assert_eq!(details(&result)["entries"][0]["matched"], json!(true));
}

#[tokio::test]
async fn paginates_matches_while_returning_full_matching_content() {
    let (_temp_dir, mut manager) = manager_fixture();
    manager
        .append_message(user_message("needle one", 1))
        .expect("append one");
    manager
        .append_message(user_message("needle two", 2))
        .expect("append two");
    let manager = Arc::new(Mutex::new(manager));

    let first_page = search(&manager, json!({ "query": "needle", "limit": 1 })).await;
    assert_eq!(details(&first_page)["totalMatches"], json!(2));
    assert_eq!(details(&first_page)["returnedMatches"], json!(1));
    assert_eq!(details(&first_page)["nextCursor"], json!("1"));
    assert_eq!(details(&first_page)["entries"][0]["content"], json!("needle one"));
    assert_eq!(details(&first_page)["entries"][0]["matched"], json!(true));

    let second_page = search(&manager, json!({ "query": "needle", "limit": 1, "cursor": "1" })).await;
    assert_eq!(details(&second_page)["totalMatches"], json!(2));
    assert_eq!(details(&second_page)["returnedMatches"], json!(1));
    assert!(details(&second_page).get("nextCursor").is_none());
    assert_eq!(details(&second_page)["entries"][0]["content"], json!("needle two"));
    assert_eq!(details(&second_page)["entries"][0]["matched"], json!(true));
}

#[tokio::test]
async fn requires_a_persisted_current_session() {
    let context = tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
        .build()
        .await
        .1;
    let error = SearchCurrentSessionHistoryTool::new()
        .execute(
            "search-history".to_owned(),
            json!(json!({ "query": "needle" })),
            None,
            None,
            Some(context),
            None,
        )
        .await
        .expect_err("unpersisted session must fail");
    assert_eq!(
        error.to_string(),
        "search_current_session_history requires a persisted current session"
    );
}

#[tokio::test]
async fn rejects_an_invalid_cursor() {
    let (_temp_dir, mut manager) = manager_fixture();
    manager
        .append_message(user_message("needle", 1))
        .expect("append message");
    let manager = Arc::new(Mutex::new(manager));
    let context = tool_context::TestContextBuilder::new("/repo", Arc::clone(&manager))
        .build()
        .await
        .1;
    let error = SearchCurrentSessionHistoryTool::new()
        .execute(
            "search-history".to_owned(),
            json!(json!({ "query": "needle", "cursor": "-1" })),
            None,
            None,
            Some(context),
            None,
        )
        .await
        .expect_err("invalid cursor must fail");
    assert_eq!(
        error.to_string(),
        "search_current_session_history cursor must be a non-negative integer cursor"
    );
}
