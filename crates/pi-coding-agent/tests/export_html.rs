//! Behavioral tests for `coding-agent/src/core/export-html/index.ts`.
//!
//! Export resolves built-in themes from `PI_PACKAGE_DIR`, so the checks run in a
//! child process with a fixture package directory.

#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/export_fixture.rs"]
mod export_fixture;

use std::path::PathBuf;

fn fixture_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/export-session.jsonl")
        .display()
        .to_string()
}

#[test]
fn exports_file_sessions_with_embedded_session_data() {
    let package = export_fixture::fixture_package_dir();
    export_fixture::run_export_probe("export_file_probe", package.path());
}

#[tokio::test]
#[ignore = "child process probe"]
async fn export_file_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::core::export_html::{ExportOptions, export_from_file};

    let dir = tempfile::tempdir().expect("temp dir");
    let output = dir.path().join("export.html");
    let written = export_from_file(
        &fixture_path(),
        Some(ExportOptions {
            output_path: Some(output.display().to_string()),
            theme_name: Some("dark".to_owned()),
            ..ExportOptions::default()
        }),
    )
    .await
    .expect("export succeeds");
    assert_eq!(written, output.display().to_string());

    let html = std::fs::read_to_string(&written).expect("read html");
    assert!(html.starts_with("<!DOCTYPE html>\n<html lang=\"en\">"));
    assert!(html.contains("--exportPageBg: #18181e;"));
    assert!(html.contains("<script id=\"session-data\" type=\"application/json\">"));

    let session = decode_session_data(&html);
    assert_eq!(session["leafId"], "m13");
    assert_eq!(session["header"]["id"], "export-fixture");
    assert_eq!(session["header"]["parentSession"], serde_json::Value::Null);
    assert_eq!(session["entries"].as_array().expect("entries").len(), 13);
    assert!(session.get("systemPrompt").is_none());
    assert!(session.get("tools").is_none());
    assert!(session.get("renderedTools").is_none());
    assert_eq!(session["entries"][0]["type"], "message");
    assert_eq!(session["entries"][0]["id"], "m1");
    assert_eq!(session["entries"][5]["type"], "compaction");
    assert_eq!(session["entries"][6]["type"], "branch_summary");
}

#[test]
fn exports_live_sessions_with_prompt_and_tools() {
    let package = export_fixture::fixture_package_dir();
    export_fixture::run_export_probe("export_session_probe", package.path());
}

#[tokio::test]
#[ignore = "child process probe"]
async fn export_session_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use std::sync::Arc;

    use pi_agent_core::agent::{
        AgentMessage, AgentState, AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError,
    };
    use pi_agent_core::harness::types::ThinkingLevel;
    use pi_ai::request::BoxFuture;
    use pi_ai::types::{Modality, Model, ModelCost};
    use pi_coding_agent::core::export_html::{ExportOptions, export_session_to_html};
    use pi_coding_agent::core::session_manager::SessionManager;

    struct FixtureTool;

    impl AgentTool for FixtureTool {
        fn name(&self) -> &str {
            "fixture"
        }

        fn description(&self) -> &str {
            "Fixture tool"
        }

        fn parameters(&self) -> &serde_json::Value {
            static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
            PARAMETERS.get_or_init(|| serde_json::json!({ "type": "object", "properties": {} }))
        }

        fn label(&self) -> &str {
            "Fixture"
        }

        fn execute(
            &self,
            _tool_call_id: String,
            _params: serde_json::Value,
            _signal: Option<tokio_util::sync::CancellationToken>,
            _on_update: AgentToolUpdateCallback,
            _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
        ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
            Box::pin(async { unreachable!("fixture tool is never executed") })
        }
    }

    let model = Model {
        id: "claude-sonnet-4-5".to_owned(),
        name: "Claude".to_owned(),
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        base_url: "https://example.invalid".to_owned(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost::default(),
        context_window: 200_000.0,
        auto_compaction_threshold: None,
        max_tokens: 8192.0,
        sampling_params: None,
        headers: None,
        compat: None,
    };
    let state = AgentState {
        system_prompt: "You are a fixture.".to_owned(),
        model,
        thinking_level: ThinkingLevel::Off,
        tools: vec![Arc::new(FixtureTool)],
        messages: Vec::<AgentMessage>::new(),
        is_streaming: false,
        is_model_request_active: false,
        streaming_message: None,
        pending_tool_calls: Default::default(),
        error_message: None,
    };

    let session = SessionManager::open(&fixture_path(), None, None).expect("open session");
    let dir = tempfile::tempdir().expect("temp dir");
    let output = dir.path().join("live.html");
    export_session_to_html(
        &session,
        Some(&state),
        Some(ExportOptions {
            output_path: Some(output.display().to_string()),
            theme_name: Some("light".to_owned()),
            ..ExportOptions::default()
        }),
    )
    .await
    .expect("export succeeds");

    let html = std::fs::read_to_string(&output).expect("read html");
    let session_data = decode_session_data(&html);
    assert_eq!(session_data["systemPrompt"], "You are a fixture.");
    let tools = session_data["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "fixture");
    assert_eq!(tools[0]["description"], "Fixture tool");
    assert_eq!(tools[0]["parameters"]["type"], "object");
}

#[test]
fn rejects_in_memory_sessions() {
    let package = export_fixture::fixture_package_dir();
    export_fixture::run_export_probe("export_in_memory_probe", package.path());
}

#[tokio::test]
#[ignore = "child process probe"]
async fn export_in_memory_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::core::export_html::export_session_to_html;
    use pi_coding_agent::core::session_manager::SessionManager;

    let session = SessionManager::in_memory(Some("/tmp"), None).expect("in-memory session");
    let error = export_session_to_html(&session, None, None)
        .await
        .expect_err("in-memory export must fail");
    assert_eq!(error.message(), "Cannot export in-memory session to HTML");
}

fn decode_session_data(html: &str) -> serde_json::Value {
    let marker = "<script id=\"session-data\" type=\"application/json\">";
    let start = html.find(marker).expect("session data script") + marker.len();
    let end = html[start..].find("</script>").expect("session data end") + start;
    let encoded = &html[start..end];
    let bytes = pi_js_compat::base64::atob(encoded).expect("session data base64");
    serde_json::from_slice(&bytes).expect("session data json")
}
