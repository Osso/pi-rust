//! Port of `packages/coding-agent/test/suite/regressions/7421-resume-session-tool.test.ts` (tool-level cases).

use std::sync::{Arc, Mutex};

use pi_agent_core::AgentHarnessTool;
use pi_coding_agent::core::extensions::{ExtensionContext, SwitchSessionCallback};
use pi_coding_agent::core::session_control_db::{WritableSessionMetadata, get_control_db_path, write_session_metadata};
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager};
use pi_coding_agent::core::tools::resume_session::ResumeSessionTool;
use serde_json::json;
use tempfile::TempDir;

#[path = "support/tool_context.rs"]
mod tool_context;

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

struct SwitchRecorder {
    cancelled: bool,
    path: Arc<Mutex<Option<String>>>,
    starter_prompt: Arc<Mutex<Option<String>>>,
}

impl SwitchRecorder {
    fn new(cancelled: bool) -> Arc<Self> {
        Arc::new(Self {
            cancelled,
            path: Arc::new(Mutex::new(None)),
            starter_prompt: Arc::new(Mutex::new(None)),
        })
    }

    fn callback(&self, context: ExtensionContext) -> SwitchSessionCallback {
        tool_context::recording_switch_session(
            context,
            self.cancelled,
            Arc::clone(&self.path),
            Arc::clone(&self.starter_prompt),
        )
    }
}

struct Fixture {
    _temp_dir: TempDir,
    session_dir: String,
    current: SessionManager,
    control_db_path: String,
}

impl Fixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let session_dir = temp_dir.path().join("sessions").display().to_string();
        let cwd = temp_dir.path().join("current").display().to_string();
        std::fs::create_dir_all(&cwd).expect("current cwd");
        let mut current = SessionManager::create(
            &cwd,
            Some(&session_dir),
            Some(NewSessionOptions {
                id: Some("current-session".to_owned()),
                ..Default::default()
            }),
        )
        .expect("current session");
        materialize(&mut current);
        let control_db_path = get_control_db_path(temp_dir.path());
        current
            .set_metadata_control_db_path(
                Some(&control_db_path),
                pi_coding_agent::core::session_manager::SetMetadataControlDbPathOptions::default(),
            )
            .expect("attach control db");
        Self {
            _temp_dir: temp_dir,
            session_dir,
            current,
            control_db_path,
        }
    }

    fn target(&self, cwd: &str, id: &str) -> SessionManager {
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
        // Real runtimes register session metadata; listing prefers it over file scans.
        manager
            .set_metadata_control_db_path(
                Some(&self.control_db_path),
                pi_coding_agent::core::session_manager::SetMetadataControlDbPathOptions::default(),
            )
            .expect("attach control db");
        manager
    }

    async fn context(&self, switcher: Option<Arc<SwitchRecorder>>, agent_id: Option<&str>) -> ExtensionContext {
        let current_path = self
            .current
            .get_session_file()
            .expect("current session file")
            .to_owned();
        let reader =
            SessionManager::open(&current_path, Some(self.session_dir.as_str()), None).expect("reopen current session");
        let mut builder = tool_context::TestContextBuilder::new(self.current.get_cwd(), Arc::new(Mutex::new(reader)))
            .control_db_path(&self.control_db_path);
        if let Some(agent_id) = agent_id {
            builder = builder.agent_id(agent_id);
        }
        let (mut runner, context) = builder.build().await;
        if let Some(recorder) = switcher {
            runner.bind_command_context(Some(tool_context::command_context_actions(
                recorder.callback(context.clone()),
            )));
        }
        context
    }
}

async fn execute(
    context: ExtensionContext,
    params: serde_json::Value,
) -> Result<pi_agent_core::AgentToolResult, pi_agent_core::ToolError> {
    ResumeSessionTool::new()
        .execute("resume-session".to_owned(), params, None, None, context, None)
        .await
}

#[tokio::test]
async fn rejects_child_agent_contexts() {
    let fixture = Fixture::new();
    let context = fixture.context(Some(SwitchRecorder::new(false)), Some("agent_1")).await;
    let error = execute(context, json!({ "path": "/tmp/x.jsonl" }))
        .await
        .expect_err("child agent must fail");
    assert_eq!(
        error.to_string(),
        "resume_session is only available from the main supervisor session"
    );
}

#[tokio::test]
async fn rejects_resuming_the_current_session_by_path_id_or_name() {
    let mut fixture = Fixture::new();
    fixture
        .current
        .set_session_name(Some("current session"))
        .expect("set name");
    let session_path = fixture
        .current
        .get_session_file()
        .expect("current session file")
        .to_owned();
    let session_id = fixture.current.get_session_id().to_owned();

    for target in [
        json!({ "path": session_path }),
        json!({ "id": session_id }),
        json!({ "name": "current session" }),
    ] {
        let error = execute(fixture.context(Some(SwitchRecorder::new(false)), None).await, target)
            .await
            .expect_err("self resume must fail");
        assert_eq!(error.to_string(), "resume_session cannot resume the current session");
    }
}

#[tokio::test]
async fn resolves_target_sessions_by_id_and_name() {
    let fixture = Fixture::new();
    let target_cwd = fixture._temp_dir.path().join("target").display().to_string();
    let mut target = fixture.target(&target_cwd, "019f7421-2000-7000-8000-000000000001");
    target
        .append_message(
            serde_json::from_value(json!({ "role": "user", "content": "target", "timestamp": 1 })).expect("message"),
        )
        .expect("append");
    target.set_session_name(Some("named target")).expect("name");
    let target_path = target.get_session_file().expect("target file").to_owned();

    let recorder = SwitchRecorder::new(false);
    let id_result = execute(
        fixture.context(Some(Arc::clone(&recorder)), None).await,
        json!({ "id": "019f7421-2000" }),
    )
    .await
    .expect("resume by id");
    assert_eq!(
        id_result.details.as_ref().expect("details"),
        &json!({ "cancelled": false, "resumed": true, "sessionPath": target_path })
    );
    assert_eq!(
        recorder.path.lock().expect("path").as_deref(),
        Some(target_path.as_str())
    );
    assert_eq!(
        serde_json::to_value(&id_result.content).expect("content"),
        json!([{ "type": "text", "text": format!("Resumed session: {target_path}") }])
    );
    assert_eq!(id_result.terminate, Some(true));

    let name_result = execute(
        fixture.context(Some(SwitchRecorder::new(false)), None).await,
        json!({ "name": "named target" }),
    )
    .await
    .expect("resume by name");
    assert_eq!(
        name_result.details.as_ref().expect("details")["sessionPath"],
        target_path
    );
}

#[tokio::test]
async fn rejects_missing_and_multiple_targets() {
    let fixture = Fixture::new();
    let error = execute(fixture.context(Some(SwitchRecorder::new(false)), None).await, json!({}))
        .await
        .expect_err("missing target must fail");
    assert_eq!(
        error.to_string(),
        "resume_session requires exactly one of path, id, or name"
    );

    let error = execute(
        fixture.context(Some(SwitchRecorder::new(false)), None).await,
        json!({ "id": "target", "path": "/tmp/session.jsonl" }),
    )
    .await
    .expect_err("multiple targets must fail");
    assert_eq!(
        error.to_string(),
        "resume_session requires exactly one of path, id, or name"
    );
}

#[tokio::test]
async fn rejects_ambiguous_named_targets() {
    let fixture = Fixture::new();
    let first_cwd = fixture._temp_dir.path().join("first").display().to_string();
    let second_cwd = fixture._temp_dir.path().join("second").display().to_string();
    let mut first = fixture.target(&first_cwd, "first-session");
    first.set_session_name(Some("duplicate target")).expect("name");
    let mut second = fixture.target(&second_cwd, "second-session");
    second.set_session_name(Some("duplicate target")).expect("name");

    let error = execute(
        fixture.context(Some(SwitchRecorder::new(false)), None).await,
        json!({ "name": "duplicate target" }),
    )
    .await
    .expect_err("ambiguous name must fail");
    assert_eq!(error.to_string(), "Ambiguous session match for name 'duplicate target'");
}

#[tokio::test]
async fn rejects_targets_whose_metadata_points_at_missing_session_files() {
    let fixture = Fixture::new();
    let stale_path = format!("{}/stale-session.jsonl", fixture.session_dir);
    write_session_metadata(
        &fixture.control_db_path,
        &WritableSessionMetadata {
            session_path: stale_path.clone(),
            id: "019f7421-stale-7000-8000-000000000001".to_owned(),
            cwd: fixture.current.get_cwd().to_owned(),
            name: Some("stale target".to_owned()),
            parent_session_path: None,
            archived_at: None,
            is_archived: None,
            is_subagent: None,
            subagent_name: None,
            model_provider: None,
            model_id: None,
            thinking_level: None,
            created_at: "2026-07-04T00:00:00.000Z".to_owned(),
            modified_at: "2026-07-04T00:00:00.000Z".to_owned(),
            message_count: 1,
            first_message: "stale".to_owned(),
        },
    )
    .expect("write metadata");

    for params in [json!({ "id": "019f7421-stale" }), json!({ "name": "stale target" })] {
        let error = execute(fixture.context(Some(SwitchRecorder::new(false)), None).await, params)
            .await
            .expect_err("stale target must fail");
        assert_eq!(error.to_string(), format!("Session file does not exist: {stale_path}"));
    }
}

#[tokio::test]
async fn resolves_id_targets_across_a_custom_session_directory() {
    let fixture = Fixture::new();
    let target_cwd = fixture._temp_dir.path().join("target").display().to_string();
    let mut target = fixture.target(&target_cwd, "019f7421-1000-7000-8000-000000000001");
    target
        .append_message(
            serde_json::from_value(json!({ "role": "user", "content": "target", "timestamp": 1 })).expect("message"),
        )
        .expect("append");
    let target_path = target.get_session_file().expect("target file").to_owned();

    let recorder = SwitchRecorder::new(false);
    let result = execute(
        fixture.context(Some(Arc::clone(&recorder)), None).await,
        json!({ "id": "019f7421-1000" }),
    )
    .await
    .expect("resume");
    assert_eq!(
        recorder.path.lock().expect("path").as_deref(),
        Some(target_path.as_str())
    );
    assert_eq!(
        result.details.as_ref().expect("details"),
        &json!({ "cancelled": false, "resumed": true, "sessionPath": target_path })
    );
}

#[tokio::test]
async fn forwards_the_starter_prompt_and_reports_cancellation() {
    let fixture = Fixture::new();
    let target_cwd = fixture._temp_dir.path().join("target").display().to_string();
    let target = fixture.target(&target_cwd, "019f7421-3000-7000-8000-000000000001");
    let target_path = target.get_session_file().expect("target file").to_owned();

    let recorder = SwitchRecorder::new(false);
    let result = execute(
        fixture.context(Some(Arc::clone(&recorder)), None).await,
        json!({ "path": target_path, "starter_prompt": "continue in target" }),
    )
    .await
    .expect("resume");
    assert_eq!(
        recorder.starter_prompt.lock().expect("starter").as_deref(),
        Some("continue in target")
    );
    assert_eq!(
        serde_json::to_value(&result.content).expect("content"),
        json!([{ "type": "text", "text": format!("Resumed session: {target_path} and sent instructions") }])
    );

    let cancelled = execute(
        fixture.context(Some(SwitchRecorder::new(true)), None).await,
        json!({ "path": target_path, "starter_prompt": "should not send" }),
    )
    .await
    .expect("cancelled resume");
    assert_eq!(
        cancelled.details.as_ref().expect("details"),
        &json!({ "cancelled": true, "resumed": false, "sessionPath": target_path })
    );
    assert_eq!(
        serde_json::to_value(&cancelled.content).expect("content"),
        json!([{ "type": "text", "text": format!("Resume cancelled: {target_path}") }])
    );
}

#[tokio::test]
async fn rejects_missing_or_non_jsonl_session_files() {
    let fixture = Fixture::new();
    let missing = fixture._temp_dir.path().join("missing.jsonl").display().to_string();
    let error = execute(
        fixture.context(Some(SwitchRecorder::new(false)), None).await,
        json!({ "path": missing }),
    )
    .await
    .expect_err("missing file must fail");
    assert_eq!(error.to_string(), format!("Session file does not exist: {missing}"));

    let text_path = fixture._temp_dir.path().join("session.txt");
    std::fs::write(&text_path, "not a session").expect("write file");
    let text_path = text_path.display().to_string();
    let error = execute(
        fixture.context(Some(SwitchRecorder::new(false)), None).await,
        json!({ "path": text_path }),
    )
    .await
    .expect_err("non-jsonl file must fail");
    assert_eq!(
        error.to_string(),
        format!("Session file must be a .jsonl file: {text_path}")
    );
}

#[tokio::test]
async fn requires_switch_session_capability() {
    let fixture = Fixture::new();
    let target_cwd = fixture._temp_dir.path().join("target").display().to_string();
    let target = fixture.target(&target_cwd, "019f7421-4000-7000-8000-000000000001");
    let target_path = target.get_session_file().expect("target file").to_owned();
    let error = execute(fixture.context(None, None).await, json!({ "path": target_path }))
        .await
        .expect_err("missing capability must fail");
    assert_eq!(
        error.to_string(),
        "Session resume is not available in this session mode"
    );
}

#[tokio::test]
async fn rejects_blank_target_strings() {
    let fixture = Fixture::new();
    let error = execute(
        fixture.context(Some(SwitchRecorder::new(false)), None).await,
        json!({ "name": "   " }),
    )
    .await
    .expect_err("blank name must fail");
    assert_eq!(error.to_string(), "resume_session name must be a non-empty string");
}
