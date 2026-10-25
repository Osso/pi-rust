//! Port of `packages/coding-agent/test/list-sessions-broadcast-tools.test.ts`.

#[path = "support/advisory_fixture.rs"]
mod advisory_fixture;
use crate::shared_oracle as oracle;
use crate::shared_tool_context as tool_context;

use pi_coding_agent::core::extensions::ExtensionContext;
use pi_coding_agent::core::extensions::types::ContextTool;
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, WritableSessionMetadata, archive_session, get_control_db_path,
    list_pending_architect_requests, list_shared_channel_messages_after, read_shared_channel_cursor,
    register_runtime_mailbox_listener, write_session_health, write_session_metadata,
};
use pi_coding_agent::core::session_health::{SessionCheckStatus, empty_session_health};
use pi_coding_agent::core::tools::ask_architect::AskArchitectTool;
use pi_coding_agent::core::tools::ask_supervisor::{AskSupervisorTool, parameters as ask_supervisor_parameters};
use pi_coding_agent::core::tools::broadcast::BroadcastTool;
use pi_coding_agent::core::tools::channel_post::ChannelPostTool;
use pi_coding_agent::core::tools::list_sessions::ListSessionsTool;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::{js_json, js_json as json};
use tempfile::TempDir;

fn write_metadata(control_db_path: &str, id: &str, session_path: &str, modified_at: &str, subagent: bool) {
    write_session_metadata(
        control_db_path,
        &WritableSessionMetadata {
            session_path: session_path.to_owned().into(),
            id: id.to_owned().into(),
            cwd: format!("/repo/{id}").into(),
            name: None,
            parent_session_path: None,
            archived_at: None,
            is_archived: None,
            is_subagent: subagent.then_some(true),
            subagent_name: None,
            model_provider: None,
            model_id: None,
            thinking_level: None,
            created_at: "2026-01-01T00:00:00.000Z".to_owned().into(),
            modified_at: modified_at.to_owned().into(),
            message_count: 1,
            first_message: "hello".to_owned().into(),
        },
    )
    .expect("write metadata");
}

fn register_listener(control_db_path: &str, session_id: &str, session_path: Option<&str>) {
    register_runtime_mailbox_listener(
        control_db_path,
        &RuntimeMailboxAddress {
            agent_id: None,
            session_id: session_id.to_owned().into(),
        },
        std::process::id(),
        session_path.map(JsString::from).as_ref(),
        Default::default(),
    )
    .expect("register listener");
}

async fn context(control_db_path: &str, session_id: &str, persisted: bool) -> ExtensionContext {
    let session = if persisted {
        let dir = tempfile::tempdir().expect("session dir");
        tool_context::persisted_session(
            "/repo",
            dir.path()
                .join("sessions")
                .to_str()
                .expect("utf-8 test path")
                .to_owned(),
            session_id,
        )
    } else {
        tool_context::in_memory_session("/repo", session_id)
    };
    tool_context::TestContextBuilder::new("/repo", session)
        .control_db_path(control_db_path)
        .build()
        .await
        .1
}

async fn subagent_context(control_db_path: &str, session_id: &str, agent_id: &str) -> ExtensionContext {
    tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", session_id))
        .control_db_path(control_db_path)
        .agent_id(agent_id)
        .build()
        .await
        .1
}

async fn execute<T: ContextTool<Option<ExtensionContext>>>(
    tool: &T,
    params: JsValue,
    context: ExtensionContext,
) -> Result<pi_agent_core::AgentToolResult, pi_agent_core::ToolError> {
    tool.execute("tool-call".to_owned(), json!(params), None, None, Some(context), None)
        .await
}

#[tokio::test]
async fn returns_only_active_and_ended_non_archived_main_sessions_with_include_ended_true() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    for (id, path, modified_at, subagent) in [
        (
            "active-main",
            "/sessions/active-main.jsonl",
            "2026-01-01T00:40:00.000Z",
            false,
        ),
        (
            "ended-main",
            "/sessions/ended-main.jsonl",
            "2026-01-01T00:30:00.000Z",
            false,
        ),
        (
            "archived-main",
            "/sessions/archived-main.jsonl",
            "2026-01-01T00:20:00.000Z",
            false,
        ),
        (
            "archived-main-child",
            "/sessions/archived-main-child.jsonl",
            "2026-01-01T00:10:00.000Z",
            true,
        ),
        (
            "historical-child",
            "/sessions/historical-child.jsonl",
            "2026-01-01T00:00:00.000Z",
            true,
        ),
    ] {
        write_metadata(&control_db_path, id, path, modified_at, subagent);
    }
    archive_session(&control_db_path, "/sessions/archived-main.jsonl").expect("archive");
    let mut health = empty_session_health("ended-main", "2026-01-01T00:00:00.000Z");
    health.agent_generation = 1;
    health.check_status = SessionCheckStatus::Dead;
    health.checked_generation = Some(1);
    write_session_health(&control_db_path, &health).expect("write health");
    register_listener(&control_db_path, "active-main", Some("/sessions/active-main.jsonl"));

    let result = execute(
        &ListSessionsTool::new(),
        json!({ "include_ended": true }),
        context(&control_db_path, "current", false).await,
    )
    .await
    .expect("list sessions");

    let details = result.details.as_ref().expect("details");
    let ids: Vec<JsString> = details["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .map(|session| session["sessionId"].as_string().expect("session id").clone())
        .collect();
    assert_eq!(ids, vec![JsString::from("active-main"), JsString::from("ended-main")]);
    assert_eq!(details["sessions"][0]["status"], json!("running"));
    assert_eq!(details["sessions"][1]["status"], json!("ended"));
    let content = js_json::to_value(&&result.content).expect("content");
    let text = content[0]["text"].as_string().expect("text");
    assert!(text.contains("active-main"));
    assert!(text.contains("ended-main"));
    assert!(!text.contains("archived-main"));
    assert!(!text.contains("archived-main-child"));
    assert!(!text.contains("historical-child"));
}

#[tokio::test]
async fn excludes_ended_rows_when_include_ended_is_false() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    write_metadata(
        &control_db_path,
        "ended",
        "/sessions/ended.jsonl",
        "2026-01-01T00:10:00.000Z",
        false,
    );
    let mut health = empty_session_health("ended", "2026-01-01T00:00:00.000Z");
    health.agent_generation = 1;
    health.check_status = SessionCheckStatus::Dead;
    health.checked_generation = Some(1);
    write_session_health(&control_db_path, &health).expect("write health");

    let result = execute(
        &ListSessionsTool::new(),
        json!({ "include_ended": false }),
        context(&control_db_path, "current", false).await,
    )
    .await
    .expect("list sessions");

    assert_eq!(result.details.as_ref().expect("details")["sessions"], json!([]));
    assert_eq!(
        js_json::to_value(&&result.content).expect("content"),
        json!([{ "type": "text", "text": "No sessions found." }])
    );
}

#[tokio::test]
async fn rejects_channel_post_from_subagent_contexts() {
    let error = execute(
        &ChannelPostTool::new(),
        json!({ "message": "hello" }),
        subagent_context("/unused", "child-session", "agent_1").await,
    )
    .await
    .expect_err("subagent channel post must fail");
    assert_eq!(error.to_string(), "channel_post is only available from main sessions");
}

#[tokio::test]
async fn channel_post_posts_the_message_and_advances_the_sender_cursor() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");

    let result = execute(
        &ChannelPostTool::new(),
        json!({ "message": "  hello  " }),
        context(&control_db_path, "main-session", false).await,
    )
    .await
    .expect("channel post");

    assert_eq!(
        js_json::to_value(&&result.content).expect("content"),
        json!([{ "type": "text", "text": "Posted shared channel message 1." }])
    );
    assert_eq!(result.details.as_ref().expect("details"), &json!({ "messageId": 1 }));
    let messages = list_shared_channel_messages_after(&control_db_path, 0, 10, i64::MAX).expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "hello");
    assert_eq!(messages[0].sender.session_id, "main-session");
    let cursor = read_shared_channel_cursor(
        &control_db_path,
        &RuntimeMailboxAddress {
            agent_id: None,
            session_id: "main-session".to_owned().into(),
        },
    )
    .expect("cursor");
    assert_eq!(cursor, Some(1));
}

#[tokio::test]
async fn channel_post_rejects_an_empty_message() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let error = execute(
        &ChannelPostTool::new(),
        json!({ "message": "   " }),
        context(&control_db_path, "main-session", false).await,
    )
    .await
    .expect_err("empty message must fail");
    assert_eq!(error.to_string(), "channel_post requires a non-empty message");
}

#[tokio::test]
async fn ask_architect_persists_the_canonical_project_path() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let project_path = temp_dir.path().join("project-worktree");
    std::fs::create_dir_all(&project_path).expect("project dir");
    let project_link = temp_dir.path().join("active-worktree");
    std::os::unix::fs::symlink(&project_path, &project_link).expect("symlink");

    let result = execute(
        &AskArchitectTool::new(),
        json!({ "message": "inspect this", "project_path": project_link }),
        context(&control_db_path, "main-session", false).await,
    )
    .await
    .expect("ask architect");

    assert_eq!(
        result.details.as_ref().expect("details")["senderSessionId"],
        json!("main-session")
    );
    let requests = list_pending_architect_requests(&control_db_path, 10).expect("requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].project_cwd.as_ref(),
        Some(&JsString::from(project_path.to_str().expect("utf-8 test path")))
    );
}

#[tokio::test]
async fn ask_architect_defaults_the_project_context_to_the_session_cwd() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    execute(
        &AskArchitectTool::new(),
        json!({ "message": "inspect this" }),
        context(&control_db_path, "main-session", false).await,
    )
    .await
    .expect("ask architect");

    let requests = list_pending_architect_requests(&control_db_path, 10).expect("requests");
    assert_eq!(requests[0].project_cwd.as_ref(), Some(&JsString::from("/repo")));
}

#[tokio::test]
async fn ask_architect_rejects_invalid_explicit_project_paths() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let file_path = temp_dir.path().join("file.txt");
    std::fs::write(&file_path, "not a directory").expect("write file");

    for project_path in ["", "../worktree"] {
        let error = execute(
            &AskArchitectTool::new(),
            json!({ "message": "inspect this", "project_path": project_path }),
            context(&control_db_path, "main-session", false).await,
        )
        .await
        .expect_err("invalid path must fail");
        assert_eq!(
            error.to_string(),
            "ask_architect project_path must be a non-empty absolute path"
        );
    }

    let error = execute(
        &AskArchitectTool::new(),
        json!({ "message": "inspect this", "project_path": temp_dir.path().join("missing") }),
        context(&control_db_path, "main-session", false).await,
    )
    .await
    .expect_err("missing path must fail");
    assert_eq!(
        error.to_string(),
        "ask_architect project_path must reference an existing directory"
    );

    let error = execute(
        &AskArchitectTool::new(),
        json!({ "message": "inspect this", "project_path": file_path }),
        context(&control_db_path, "main-session", false).await,
    )
    .await
    .expect_err("file path must fail");
    assert_eq!(
        error.to_string(),
        "ask_architect project_path must reference a directory"
    );
}

#[tokio::test]
async fn ask_architect_rejects_subagent_runtimes() {
    let error = execute(
        &AskArchitectTool::new(),
        json!({ "message": "inspect this" }),
        subagent_context("/unused", "child-session", "agent_1").await,
    )
    .await
    .expect_err("subagent architect request must fail");
    assert_eq!(error.to_string(), "ask_architect is only available from main sessions");
}

#[test]
fn ask_supervisor_bounds_advisory_input() {
    let schema = ask_supervisor_parameters();
    assert_eq!(schema["properties"]["question"]["maxLength"], 4_000);
    assert_eq!(schema["properties"]["context"]["maxLength"], 8_000);
}

#[tokio::test]
async fn ask_supervisor_rejects_subagent_runtimes() {
    let error = execute(
        &AskSupervisorTool::new(),
        json!({ "question": "Is this complete?" }),
        subagent_context("/unused", "child-session", "agent_1").await,
    )
    .await
    .expect_err("subagent advisory must fail");
    assert_eq!(error.to_string(), "ask_supervisor is only available from main sessions");
}

#[test]
fn ask_supervisor_returns_the_advisory_answer() {
    // The private child uses a ready resident IPC endpoint and real durable requests.
    // Launching this libtest executable with `supervisor` would rerun this test.
    advisory_fixture::run_isolated();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private KB namespace probe invoked by ask_supervisor_returns_the_advisory_answer"]
async fn isolated_advisory_probe() {
    advisory_fixture::run_probe().await;
}

#[tokio::test]
async fn broadcast_requires_a_persisted_session_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let error = execute(
        &BroadcastTool::new(),
        json!({ "message": "please restart" }),
        context(&control_db_path, "sender", false).await,
    )
    .await
    .expect_err("missing session file must fail");
    assert_eq!(error.to_string(), "broadcast requires a persisted session file");
}

#[tokio::test]
async fn broadcast_rejects_an_empty_message() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((temp_dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let error = execute(
        &BroadcastTool::new(),
        json!({ "message": "   " }),
        context(&control_db_path, "sender", true).await,
    )
    .await
    .expect_err("empty message must fail");
    assert_eq!(error.to_string(), "broadcast requires a non-empty message");
}

#[tokio::test]
async fn list_sessions_requires_a_control_database_path() {
    let error = ListSessionsTool::new()
        .execute(
            "tool-call".to_owned(),
            json!(json!({})),
            None,
            None,
            Some(
                tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
                    .build()
                    .await
                    .1,
            ),
            None,
        )
        .await
        .expect_err("missing control db must fail");
    assert_eq!(error.to_string(), "list_sessions requires a control database path");
}

#[test]
fn coordination_tools_expose_their_source_metadata() {
    assert_eq!(ListSessionsTool::new().name(), "list_sessions");
    assert!(ListSessionsTool::new().description().contains("sticky liveness"));
    assert!(BroadcastTool::new().description().contains("eligible"));
    assert!(ChannelPostTool::new().description().contains("shared channel"));
    assert_eq!(AskSupervisorTool::new().name(), "ask_supervisor");
    assert_eq!(AskArchitectTool::new().name(), "ask_architect");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/tool_context.rs"]
pub mod shared_tool_context;
