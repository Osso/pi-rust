//! Port of `packages/coding-agent/test/suite/change-working-directory-tool.test.ts` (tool-level cases).

use std::sync::{Arc, Mutex};

use pi_coding_agent::core::extensions::types::ContextTool;
use pi_coding_agent::core::extensions::{ExtensionContext, ExtensionRunner, RelocateCallback};
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager};
use pi_coding_agent::core::tools::change_working_directory::ChangeWorkingDirectoryTool;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::{js_json, js_json as json};
use tempfile::TempDir;

fn seed_message(text: &str, timestamp: i64) -> pi_agent_core::AgentMessage {
    js_json::from_value(&json!({ "role": "user", "content": text, "timestamp": timestamp })).expect("seed user message")
}

fn seed_assistant(text: &str, timestamp: i64) -> pi_agent_core::AgentMessage {
    let mut message = pi_ai::types::AssistantMessage::empty("openai-responses", "openai", "gpt-5.5", timestamp as f64);
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
    calls: Mutex<Vec<JsString>>,
}

fn relocate_callback(recorder: Arc<RelocateRecorder>) -> RelocateCallback {
    Arc::new(move |target_cwd| {
        let recorder = Arc::clone(&recorder);
        let target_cwd = target_cwd.to_owned();
        Box::pin(async move {
            recorder.calls.lock().expect("calls lock").push(target_cwd);
            Ok(())
        })
    })
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
        let session_dir = temp_dir
            .path()
            .join("sessions")
            .to_str()
            .expect("utf-8 test path")
            .to_owned();
        let root_cwd = temp_dir
            .path()
            .join("root")
            .to_str()
            .expect("utf-8 test path")
            .to_owned();
        std::fs::create_dir_all(&root_cwd).expect("root cwd");
        let mut current = SessionManager::create(
            &JsString::from(&root_cwd),
            Some(&JsString::from(&session_dir)),
            Some(NewSessionOptions {
                id: Some("019f8def-4d62-7d88-9690-9e450ee71d64".to_owned().into()),
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

    fn target_session(&self, cwd: &str, id: impl Into<JsString>) -> SessionManager {
        std::fs::create_dir_all(cwd).expect("target cwd");
        let mut manager = SessionManager::create(
            &JsString::from(cwd),
            Some(&JsString::from(&self.session_dir)),
            Some(NewSessionOptions {
                id: Some(id.into()),
                ..Default::default()
            }),
        )
        .expect("target session");
        materialize(&mut manager);
        manager
    }

    async fn context(&self, relocate: Option<Arc<RelocateRecorder>>) -> (ExtensionRunner, ExtensionContext) {
        let current_path = self
            .current
            .get_session_file()
            .expect("current session file")
            .to_owned();
        let reader = SessionManager::open(&current_path, Some(&JsString::from(self.session_dir.as_str())), None)
            .expect("reopen current session");
        let (mut runner, context) =
            crate::tool_context::TestContextBuilder::new(&self.root_cwd, Arc::new(Mutex::new(reader)))
                .build()
                .await;
        if let Some(recorder) = relocate {
            runner.set_tool_result_relocation_handlers(
                relocate_callback(recorder),
                Arc::new(|| Box::pin(async { Ok(()) })),
            );
        }
        (runner, context)
    }
}

use crate::shared_tool_context as tool_context;

async fn execute(
    context: ExtensionContext,
    params: JsValue,
) -> Result<pi_agent_core::AgentToolResult, pi_agent_core::ToolError> {
    ChangeWorkingDirectoryTool::new()
        .execute("change-cwd".to_owned(), json!(params), None, None, Some(context), None)
        .await
}

#[tokio::test]
async fn changes_the_current_session_cwd_from_a_direct_directory_path() {
    let fixture = Fixture::new();
    let target_cwd = fixture.temp_dir.path().join("direct-target");
    std::fs::create_dir_all(&target_cwd).expect("target dir");
    let target_cwd = target_cwd.to_str().expect("utf-8 test path").to_owned();
    let recorder = Arc::new(RelocateRecorder::default());
    let (runner, context) = fixture.context(Some(Arc::clone(&recorder))).await;

    let result = execute(context, json!({ "path": target_cwd }))
        .await
        .expect("change cwd");
    runner
        .deliver_tool_result_relocation("change-cwd")
        .await
        .expect("deliver relocation");

    assert_eq!(
        recorder.calls.lock().expect("calls").as_slice(),
        std::slice::from_ref(&target_cwd)
    );
    assert_eq!(
        result.details.as_ref().expect("details"),
        &json!({ "previousCwd": fixture.root_cwd, "cwd": target_cwd, "source": "path" })
    );
    assert_eq!(
        js_json::to_value(&&result.content).expect("content"),
        json!([{ "type": "text", "text": format!("Changed working directory to {target_cwd}") }])
    );
    assert_eq!(result.terminate, Some(true));
}

#[tokio::test]
async fn schedules_relocation_only_for_its_tool_call_id() {
    let fixture = Fixture::new();
    let target_cwd = fixture.temp_dir.path().join("direct-target");
    std::fs::create_dir_all(&target_cwd).expect("target dir");
    let target_cwd = target_cwd.to_str().expect("utf-8 test path").to_owned();
    let recorder = Arc::new(RelocateRecorder::default());
    let (runner, context) = fixture.context(Some(Arc::clone(&recorder))).await;

    execute(context, json!({ "path": target_cwd }))
        .await
        .expect("change cwd");
    runner
        .deliver_tool_result_relocation("other-call")
        .await
        .expect("deliver other call");
    assert!(recorder.calls.lock().expect("calls").is_empty());

    runner
        .deliver_tool_result_relocation("change-cwd")
        .await
        .expect("deliver relocation");
    assert_eq!(
        recorder.calls.lock().expect("calls").as_slice(),
        std::slice::from_ref(&target_cwd)
    );
}

#[tokio::test]
async fn preserves_whitespace_in_a_direct_directory_path() {
    let fixture = Fixture::new();
    let target_cwd = fixture.temp_dir.path().join(" target with whitespace ");
    std::fs::create_dir_all(&target_cwd).expect("target dir");
    let target_cwd = target_cwd.to_str().expect("utf-8 test path").to_owned();
    let recorder = Arc::new(RelocateRecorder::default());
    let (runner, context) = fixture.context(Some(Arc::clone(&recorder))).await;

    execute(context, json!({ "path": target_cwd }))
        .await
        .expect("change cwd");
    runner
        .deliver_tool_result_relocation("change-cwd")
        .await
        .expect("deliver relocation");

    assert_eq!(recorder.calls.lock().expect("calls")[0], target_cwd);
}

#[tokio::test]
async fn resolves_a_session_id_to_its_recorded_cwd() {
    let fixture = Fixture::new();
    let target_cwd = fixture
        .temp_dir
        .path()
        .join("tradebot")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let target = fixture.target_session(&target_cwd, "019f8def-1111-7000-8000-000000000001");
    let target_session_file = target.get_session_file().expect("target file").to_owned();
    let recorder = Arc::new(RelocateRecorder::default());
    let (runner, context) = fixture.context(Some(Arc::clone(&recorder))).await;

    let result = execute(context, json!({ "id": target.get_session_id() }))
        .await
        .expect("change cwd");
    runner
        .deliver_tool_result_relocation("change-cwd")
        .await
        .expect("deliver relocation");

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
        std::slice::from_ref(&target_cwd)
    );
    assert_eq!(fixture.current.get_session_id(), "019f8def-4d62-7d88-9690-9e450ee71d64");
    assert_ne!(
        fixture.current.get_session_file().expect("current file"),
        &target_session_file
    );
}

#[tokio::test]
async fn rejects_a_current_session_id_prefix() {
    let fixture = Fixture::new();
    let prefix = &fixture.current.get_session_id().slice(0, 8);
    let (_, context) = fixture.context(None).await;
    let error = execute(context, json!({ "id": prefix }))
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
    let short_id = &fixture.current.get_session_id().slice(0, 8);
    let target_cwd = fixture
        .temp_dir
        .path()
        .join("short-id-target")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    fixture.target_session(&target_cwd, short_id);
    let recorder = Arc::new(RelocateRecorder::default());
    let (runner, context) = fixture.context(Some(Arc::clone(&recorder))).await;

    execute(context, json!({ "id": short_id })).await.expect("change cwd");
    runner
        .deliver_tool_result_relocation("change-cwd")
        .await
        .expect("deliver relocation");

    assert_eq!(recorder.calls.lock().expect("calls")[0], target_cwd);
}

#[tokio::test]
async fn rejects_a_missing_directory() {
    let fixture = Fixture::new();
    let missing_cwd = fixture
        .temp_dir
        .path()
        .join("missing")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let (_, context) = fixture.context(None).await;
    let error = execute(context, json!({ "path": missing_cwd }))
        .await
        .expect_err("missing dir must fail");
    assert_eq!(error.to_string(), format!("Directory does not exist: {missing_cwd}"));
}

#[tokio::test]
async fn rejects_a_non_directory_path() {
    let fixture = Fixture::new();
    let file_path = fixture.temp_dir.path().join("file.txt");
    std::fs::write(&file_path, "not a directory").expect("write file");
    let file_path = file_path.to_str().expect("utf-8 test path").to_owned();
    let (_, context) = fixture.context(None).await;
    let error = execute(context, json!({ "path": file_path }))
        .await
        .expect_err("file path must fail");
    assert_eq!(error.to_string(), format!("Not a directory: {file_path}"));
}

#[tokio::test]
async fn requires_exactly_one_of_path_or_id() {
    let fixture = Fixture::new();
    for params in [json!({}), json!({ "path": fixture.root_cwd, "id": "target" })] {
        let (_, context) = fixture.context(None).await;
        let error = execute(context, params).await.expect_err("invalid target must fail");
        assert_eq!(
            error.to_string(),
            "change_working_directory requires exactly one of path or id"
        );
    }

    let (_, context) = fixture.context(None).await;
    let error = execute(context, json!({ "path": "   " }))
        .await
        .expect_err("blank path must fail");
    assert_eq!(
        error.to_string(),
        "change_working_directory path must be a non-empty string"
    );
}

// Shared support is compiled once per integration target.
#[path = "support/tool_context.rs"]
pub mod shared_tool_context;
