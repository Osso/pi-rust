//! Port of `packages/coding-agent/test/suite/change-working-directory-tool.test.ts` (tool-level cases).

use std::sync::{Arc, Mutex};

use pi_agent_core::AgentHarnessTool;
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager};
use pi_coding_agent::core::tools::change_working_directory::ChangeWorkingDirectoryTool;
use pi_coding_agent::core::tools::context::{RelocateAfterToolResult, ToolContext};
use serde_json::json;
use tempfile::TempDir;

fn seed_message(text: &str, timestamp: i64) -> pi_agent_core::AgentMessage {
    serde_json::from_value(json!({ "role": "user", "content": text, "timestamp": timestamp }))
        .expect("seed user message")
}

fn seed_assistant(text: &str, timestamp: i64) -> pi_agent_core::AgentMessage {
    let mut message = pi_ai::types::AssistantMessage::empty("openai-responses", "openai", "gpt-5.5", timestamp);
    message
        .content
        .push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
            text,
        )));
    pi_agent_core::AgentMessage::Assistant(Box::new(message))
}

fn materialize(manager: &mut SessionManager) {
    manager
        .append_message(seed_message("seed", 1))
        .expect("append seed user");
    manager
        .append_message(seed_assistant("seed reply", 2))
        .expect("append seed assistant");
}

#[derive(Default)]
struct RelocateRecorder {
    calls: Mutex<Vec<(String, String)>>,
}

impl RelocateAfterToolResult for RelocateRecorder {
    fn relocate_after_tool_result(&self, tool_call_id: &str, target_cwd: &str) {
        self.calls
            .lock()
            .expect("calls lock")
            .push((tool_call_id.to_owned(), target_cwd.to_owned()));
    }
}

struct Fixture {
    temp_dir: TempDir,
    session_dir: String,
    root_cwd: String,
    current: SessionManager,
}

impl Fixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let session_dir = temp_dir.path().join("sessions").display().to_string();
        let root_cwd = temp_dir.path().join("root").display().to_string();
        std::fs::create_dir_all(&root_cwd).expect("root cwd");
        let mut current = SessionManager::create(
            &root_cwd,
            Some(&session_dir),
            Some(NewSessionOptions {
                id: Some("019f8def-4d62-7d88-9690-9e450ee71d64".to_owned()),
                ..Default::default()
            }),
        )
        .expect("current session");
        materialize(&mut current);
        Self {
            temp_dir,
            session_dir,
            root_cwd,
            current,
        }
    }

    fn target_session(&self, cwd: &str, id: &str) -> SessionManager {
        std::fs::create_dir_all(cwd).expect("target cwd");
        let mut manager = SessionManager::create(
            cwd,
            Some(&self.session_dir),
            Some(NewSessionOptions {
                id: Some(id.to_owned()),
                ..Default::default()
            }),
        )
        .expect("target session");
        materialize(&mut manager);
        manager
    }

    fn context(&self, relocate: Option<Arc<dyn RelocateAfterToolResult>>) -> ToolContext {
        let current_path = self
            .current
            .get_session_file()
            .expect("current session file")
            .to_owned();
        let reader =
            SessionManager::open(&current_path, Some(self.session_dir.as_str()), None).expect("reopen current session");
        let context = ToolContext::new(&self.root_cwd, Arc::new(reader));
        match relocate {
            Some(relocate) => context.with_relocate_after_tool_result(relocate),
            None => context,
        }
    }
}

async fn execute(
    context: ToolContext,
    params: serde_json::Value,
) -> Result<pi_agent_core::AgentToolResult, pi_agent_core::ToolError> {
    ChangeWorkingDirectoryTool::new()
        .execute("change-cwd".to_owned(), params, None, None, context, None)
        .await
}

#[tokio::test]
async fn changes_the_current_session_cwd_from_a_direct_directory_path() {
    let fixture = Fixture::new();
    let target_cwd = fixture.temp_dir.path().join("direct-target");
    std::fs::create_dir_all(&target_cwd).expect("target dir");
    let target_cwd = target_cwd.display().to_string();
    let recorder = Arc::new(RelocateRecorder::default());

    let result = execute(fixture.context(Some(recorder.clone())), json!({ "path": target_cwd }))
        .await
        .expect("change cwd");

    assert_eq!(
        recorder.calls.lock().expect("calls").as_slice(),
        &[("change-cwd".to_owned(), target_cwd.clone())]
    );
    assert_eq!(
        result.details.as_ref().expect("details"),
        &json!({ "previousCwd": fixture.root_cwd, "cwd": target_cwd, "source": "path" })
    );
    assert_eq!(
        serde_json::to_value(&result.content).expect("content"),
        json!([{ "type": "text", "text": format!("Changed working directory to {target_cwd}") }])
    );
    assert_eq!(result.terminate, Some(true));
}

#[tokio::test]
async fn preserves_whitespace_in_a_direct_directory_path() {
    let fixture = Fixture::new();
    let target_cwd = fixture.temp_dir.path().join(" target with whitespace ");
    std::fs::create_dir_all(&target_cwd).expect("target dir");
    let target_cwd = target_cwd.display().to_string();
    let recorder = Arc::new(RelocateRecorder::default());

    execute(fixture.context(Some(recorder.clone())), json!({ "path": target_cwd }))
        .await
        .expect("change cwd");

    assert_eq!(recorder.calls.lock().expect("calls")[0].1, target_cwd);
}

#[tokio::test]
async fn resolves_a_session_id_to_its_recorded_cwd() {
    let fixture = Fixture::new();
    let target_cwd = fixture.temp_dir.path().join("tradebot").display().to_string();
    let target = fixture.target_session(&target_cwd, "019f8def-1111-7000-8000-000000000001");
    let target_session_file = target.get_session_file().expect("target file").to_owned();
    let recorder = Arc::new(RelocateRecorder::default());

    let result = execute(
        fixture.context(Some(recorder.clone())),
        json!({ "id": target.get_session_id() }),
    )
    .await
    .expect("change cwd");

    assert_eq!(
        result.details.as_ref().expect("details"),
        &json!({
            "previousCwd": fixture.root_cwd,
            "cwd": target_cwd,
            "source": "session",
            "sessionId": target.get_session_id()
        })
    );
    assert_eq!(
        recorder.calls.lock().expect("calls").as_slice(),
        &[("change-cwd".to_owned(), target_cwd)]
    );
    assert_eq!(fixture.current.get_session_id(), "019f8def-4d62-7d88-9690-9e450ee71d64");
    assert_ne!(
        fixture.current.get_session_file().expect("current file"),
        target_session_file
    );
}

#[tokio::test]
async fn rejects_a_current_session_id_prefix() {
    let fixture = Fixture::new();
    let prefix = &fixture.current.get_session_id()[..8];
    let error = execute(
        fixture.context(Some(Arc::new(RelocateRecorder::default()))),
        json!({ "id": prefix }),
    )
    .await
    .expect_err("current session prefix must fail");
    assert_eq!(
        error.to_string(),
        "current session id cannot be used as a working directory target"
    );
}

#[tokio::test]
async fn prefers_an_exact_shorter_session_id_over_a_current_session_prefix() {
    let fixture = Fixture::new();
    let short_id = &fixture.current.get_session_id()[..8];
    let target_cwd = fixture.temp_dir.path().join("short-id-target").display().to_string();
    fixture.target_session(&target_cwd, short_id);
    let recorder = Arc::new(RelocateRecorder::default());

    execute(fixture.context(Some(recorder.clone())), json!({ "id": short_id }))
        .await
        .expect("change cwd");

    assert_eq!(recorder.calls.lock().expect("calls")[0].1, target_cwd);
}

#[tokio::test]
async fn rejects_a_missing_directory() {
    let fixture = Fixture::new();
    let missing_cwd = fixture.temp_dir.path().join("missing").display().to_string();
    let error = execute(
        fixture.context(Some(Arc::new(RelocateRecorder::default()))),
        json!({ "path": missing_cwd }),
    )
    .await
    .expect_err("missing dir must fail");
    assert_eq!(error.to_string(), format!("Directory does not exist: {missing_cwd}"));
}

#[tokio::test]
async fn rejects_a_non_directory_path() {
    let fixture = Fixture::new();
    let file_path = fixture.temp_dir.path().join("file.txt");
    std::fs::write(&file_path, "not a directory").expect("write file");
    let file_path = file_path.display().to_string();
    let error = execute(
        fixture.context(Some(Arc::new(RelocateRecorder::default()))),
        json!({ "path": file_path }),
    )
    .await
    .expect_err("file path must fail");
    assert_eq!(error.to_string(), format!("Not a directory: {file_path}"));
}

#[tokio::test]
async fn requires_relocation_capability() {
    let fixture = Fixture::new();
    let error = execute(fixture.context(None), json!({ "path": fixture.root_cwd }))
        .await
        .expect_err("missing capability must fail");
    assert_eq!(
        error.to_string(),
        "change_working_directory is not available in this session mode"
    );
}

#[tokio::test]
async fn requires_exactly_one_of_path_or_id() {
    let fixture = Fixture::new();
    for params in [json!({}), json!({ "path": fixture.root_cwd, "id": "target" })] {
        let error = execute(fixture.context(Some(Arc::new(RelocateRecorder::default()))), params)
            .await
            .expect_err("invalid target must fail");
        assert_eq!(
            error.to_string(),
            "change_working_directory requires exactly one of path or id"
        );
    }

    let error = execute(
        fixture.context(Some(Arc::new(RelocateRecorder::default()))),
        json!({ "path": "   " }),
    )
    .await
    .expect_err("blank path must fail");
    assert_eq!(
        error.to_string(),
        "change_working_directory path must be a non-empty string"
    );
}
