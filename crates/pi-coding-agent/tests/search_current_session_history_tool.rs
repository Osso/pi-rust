//! Port of `packages/coding-agent/test/search-current-session-history-tool.test.ts`.

#[path = "support/tool_context.rs"]
mod tool_context;

use std::sync::Arc;

use pi_agent_core::{AgentHarnessTool, AgentMessage};
use pi_ai::types::{AssistantContent, AssistantMessage, TextContent};
use pi_coding_agent::core::session_manager::{CompactionAppendOptions, SessionManager};
use pi_coding_agent::core::tools::search_current_session_history::SearchCurrentSessionHistoryTool;
use serde_json::{Value, json};
use tempfile::TempDir;

fn manager_fixture() -> (TempDir, SessionManager) {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let manager = SessionManager::create(
        "/repo",
        Some(&temp_dir.path().join("sessions").display().to_string()),
        None,
    )
    .expect("session manager");
    (temp_dir, manager)
}

fn user_message(text: &str, timestamp: i64) -> AgentMessage {
    serde_json::from_value(json!({ "role": "user", "content": text, "timestamp": timestamp })).expect("user message")
}

fn assistant_message(text: &str, timestamp: i64) -> AgentMessage {
    let mut message = AssistantMessage::empty("openai-responses", "openai", "gpt-5.5", timestamp);
    message.content.push(AssistantContent::Text(TextContent::new(text)));
    AgentMessage::Assistant(Box::new(message))
}

async fn search(manager: &Arc<SessionManager>, params: Value) -> pi_agent_core::AgentToolResult {
    let context = tool_context::TestContextBuilder::new("/repo", Arc::clone(manager))
        .build()
        .await
        .1;
    SearchCurrentSessionHistoryTool::new()
        .execute("search-history".to_owned(), params, None, None, context, None)
        .await
        .expect("search history")
}

fn details(result: &pi_agent_core::AgentToolResult) -> &Value {
    result.details.as_ref().expect("details")
}

#[tokio::test]
async fn matches_user_messages_without_searching_assistant_or_tool_content() {
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
    let manager = Arc::new(manager);

    let user_match = search(&manager, json!({ "query": "list-skills" })).await;
    let assistant_match = search(&manager, json!({ "query": "reddit" })).await;

    assert_eq!(details(&user_match)["totalMatches"], 1);
    assert_eq!(details(&user_match)["entries"][0]["role"], "user");
    assert_eq!(details(&user_match)["entries"][0]["content"], "list-skills");
    assert_eq!(details(&user_match)["entries"][0]["matched"], true);
    assert_eq!(details(&assistant_match)["totalMatches"], 0);
    assert_eq!(details(&assistant_match)["entries"], json!([]));
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
            "Earlier deployment discussion",
            &kept_id,
            1000.0,
            CompactionAppendOptions::default(),
        )
        .expect("append compaction");
    manager
        .append_message(user_message("Current follow-up", 3))
        .expect("append follow-up");
    let manager = Arc::new(manager);

    let result = search(&manager, json!({ "query": "COBALT", "context_entries": 1 })).await;

    assert_eq!(details(&result)["totalMatches"], 1);
    let entries = details(&result)["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["id"], first_id);
    assert_eq!(entries[0]["matched"], true);
    assert_eq!(entries[0]["compacted"], true);
    assert_eq!(entries[0]["role"], "user");
    assert_eq!(entries[1]["matched"], false);
    assert_eq!(entries[1]["content"], "Neighbor before compaction");
    assert_eq!(entries[1]["role"], "user");
    let content = serde_json::to_value(&result.content).expect("content");
    assert!(
        content[0]["text"]
            .as_str()
            .expect("text")
            .contains("Original deployment used cobalt")
    );
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
    let manager = Arc::new(manager);

    let result = search(&manager, json!({ "query": "inactive branch secret" })).await;

    assert_eq!(details(&result)["totalMatches"], 0);
    assert_eq!(details(&result)["entries"], json!([]));
    assert_eq!(
        serde_json::to_value(&result.content).expect("content"),
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
    let manager = Arc::new(manager);

    let result = search(&manager, json!({ "query": content })).await;

    assert_eq!(details(&result)["totalMatches"], 1);
    assert_eq!(details(&result)["entries"][0]["content"], content);
    assert_eq!(details(&result)["entries"][0]["matched"], true);
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
    let manager = Arc::new(manager);

    let first_page = search(&manager, json!({ "query": "needle", "limit": 1 })).await;
    assert_eq!(details(&first_page)["totalMatches"], 2);
    assert_eq!(details(&first_page)["returnedMatches"], 1);
    assert_eq!(details(&first_page)["nextCursor"], "1");
    assert_eq!(details(&first_page)["entries"][0]["content"], "needle one");
    assert_eq!(details(&first_page)["entries"][0]["matched"], true);

    let second_page = search(&manager, json!({ "query": "needle", "limit": 1, "cursor": "1" })).await;
    assert_eq!(details(&second_page)["totalMatches"], 2);
    assert_eq!(details(&second_page)["returnedMatches"], 1);
    assert!(details(&second_page).get("nextCursor").is_none());
    assert_eq!(details(&second_page)["entries"][0]["content"], "needle two");
    assert_eq!(details(&second_page)["entries"][0]["matched"], true);
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
            json!({ "query": "needle" }),
            None,
            None,
            context,
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
    let manager = Arc::new(manager);
    let context = tool_context::TestContextBuilder::new("/repo", Arc::clone(&manager))
        .build()
        .await
        .1;
    let error = SearchCurrentSessionHistoryTool::new()
        .execute(
            "search-history".to_owned(),
            json!({ "query": "needle", "cursor": "-1" }),
            None,
            None,
            context,
            None,
        )
        .await
        .expect_err("invalid cursor must fail");
    assert_eq!(
        error.to_string(),
        "search_current_session_history cursor must be a non-negative integer cursor"
    );
}
