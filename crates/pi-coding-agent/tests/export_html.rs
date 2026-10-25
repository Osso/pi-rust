//! Behavioral tests for `coding-agent/src/core/export-html/index.ts`.
//!
//! Export resolves built-in themes from `PI_PACKAGE_DIR`, so the checks run in a
//! child process with a fixture package directory.

#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/export_fixture.rs"]
mod export_fixture;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
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
        &JsString::from(&fixture_path()),
        Some(ExportOptions {
            output_path: Some(JsString::from(output.display().to_string())),
            theme_name: Some(JsString::from("dark")),
            ..ExportOptions::default()
        }),
    )
    .await
    .expect("export succeeds");
    assert_eq!(written, output.display().to_string());

    let html = std::fs::read_to_string(&&encode_node_utf8(&written)).expect("read html");
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

        fn description(&self) -> pi_js_compat::js_json::JsString {
            "Fixture tool".into()
        }

        fn parameters(&self) -> &serde_json::Value {
            static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
            PARAMETERS.get_or_init(|| serde_json::json!({ "type": "object", "properties": {} }))
        }

        fn label(&self) -> pi_js_compat::js_json::JsString {
            "Fixture".into()
        }

        fn execute(
            &self,
            _tool_call_id: String,
            _params: JsValue,
            _signal: Option<pi_js_compat::abort::AbortSignal>,
            _on_update: AgentToolUpdateCallback,
            _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
        ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
            Box::pin(async { unreachable!("fixture tool is never executed") })
        }
    }

    let model = Model {
        properties: Default::default(),
        id: "claude-sonnet-4-5".to_owned(),
        name: "Claude".into(),
        api: "anthropic-messages".to_owned(),
        provider: "anthropic".to_owned(),
        base_url: JsString::from("https://example.invalid"),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost::default(),
        context_window: 200_000.0,
        auto_compaction_threshold: None,
        max_tokens: 8192.0,
        headers: None,
        compat: None,
    };
    let state = AgentState {
        system_prompt: JsString::from("You are a fixture."),
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

    let session = SessionManager::open(&JsString::from(&fixture_path()), None, None).expect("open session");
    let dir = tempfile::tempdir().expect("temp dir");
    let output = dir.path().join("live.html");
    export_session_to_html(
        &session,
        Some(&state),
        Some(ExportOptions {
            output_path: Some(JsString::from(output.display().to_string())),
            theme_name: Some(JsString::from("light")),
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

    let session = SessionManager::in_memory(Some(&JsString::from("/tmp")), None).expect("in-memory session");
    let error = export_session_to_html(&session, None, None)
        .await
        .expect_err("in-memory export must fail");
    assert_eq!(error.message(), "Cannot export in-memory session to HTML");
}

#[test]
fn exports_agent_sessions_through_session_api() {
    let package = export_fixture::fixture_package_dir();
    export_fixture::run_export_probe("export_agent_session_probe", package.path());
}

#[tokio::test]
#[ignore = "child process probe"]
async fn export_agent_session_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use std::sync::Arc;

    use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
    use pi_agent_core::harness::types::ThinkingLevel;
    use pi_ai::providers::faux::{FauxModelDefinition, RegisterFauxProviderOptions, faux_provider};
    use pi_coding_agent::core::agent_session::{AgentSession, AgentSessionConfig};
    use pi_coding_agent::core::auth_storage::AuthStorage;
    use pi_coding_agent::core::model_registry::ModelRegistry;
    use pi_coding_agent::core::session_manager::SessionManager;
    use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};

    pi_coding_agent::modes::interactive::theme::init_theme(Some(&JsString::from("dark")), false).expect("dark theme");

    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".into()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    });
    let mut credentials = serde_json::Map::new();
    credentials.insert(
        "faux".to_owned(),
        serde_json::json!({"type": "api_key", "key": "faux-key"}),
    );
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(credentials)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    let model = faux.get_model(None).expect("faux model");
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            model: Some(model),
            system_prompt: Some(JsString::default()),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(Vec::new()),
            messages: None,
        }),
        stream_fn: Some(Arc::new(move |model, context, options| {
            Box::pin(async move {
                pi_ai::compat::stream_simple(model, context, options).map_err(pi_agent_core::AgentError::from)
            })
        })),
        ..Default::default()
    })
    .expect("agent");
    let session_manager = SessionManager::open(&JsString::from(&fixture_path()), None, None).expect("open session");
    let mut settings = Settings::new();
    settings.insert(JsString::from("theme"), JsValue::from(serde_json::json!("dark")));
    let settings_manager = SettingsManager::in_memory(settings, SettingsManagerCreateOptions::default());
    let dir = tempfile::tempdir().expect("temp dir");
    let session = AgentSession::new(AgentSessionConfig {
        agent,
        session_manager: Arc::new(std::sync::Mutex::new(session_manager)),
        settings_manager: Arc::new(std::sync::Mutex::new(settings_manager)),
        cwd: JsString::from("/tmp"),
        scoped_models: Vec::new(),
        resource_loader: pi_coding_agent::core::resource_loader::StaticResourceLoader::new(),
        model_registry: runtime,
        initial_active_tool_names: None,
        allowed_tool_names: None,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(JsString::from("/tmp")),
        control_db_path: Some(JsString::from(dir.path().join("control.db").display().to_string())),
        base_tools: Vec::new(),
        custom_tool_definitions: Vec::new(),
        custom_tools: Vec::new(),
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: None,
        no_supervisor: None,
        supervisor_decision_requester: None,
        multi_agent_store: None,
        multi_agent_agent_id: None,
        multi_agent_parent_session_id: None,
        multi_agent_requires_agent_id: false,
        disable_runtime_coordination_inbound: false,
        on_shared_channel_message_delivered: None,
        extension_runner_ref: None,
        session_start_event: None,
    })
    .expect("session");
    assert!(!session.extension_runner().has_handlers("session_start"));

    let output = dir.path().join("session.html");
    let written = session
        .export_to_html(Some(&JsString::from(&output.display().to_string())))
        .await
        .expect("export succeeds");
    assert_eq!(written, output.display().to_string());
    let html = std::fs::read_to_string(&&encode_node_utf8(&written)).expect("read html");
    assert!(html.contains("--exportPageBg: #18181e;"));
    let session_data = decode_session_data(&html);
    assert_eq!(session_data["header"]["id"], "export-fixture");
    assert_eq!(session_data["entries"].as_array().expect("entries").len(), 13);
}

fn decode_session_data(html: &str) -> serde_json::Value {
    let marker = "<script id=\"session-data\" type=\"application/json\">";
    let start = html.find(marker).expect("session data script") + marker.len();
    let end = html[start..].find("</script>").expect("session data end") + start;
    let encoded = &html[start..end];
    let bytes = pi_js_compat::base64::atob(&encoded.into()).expect("session data base64");
    serde_json::from_slice(&bytes).expect("session data json")
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
