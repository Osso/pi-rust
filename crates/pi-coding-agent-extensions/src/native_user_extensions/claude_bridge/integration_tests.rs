use super::*;
use crate::core::extensions::{ExtensionRegistry, types::ProviderConfig};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{
    AssistantContent, Context, Message, StopReason, Tool, ToolResultMessage, ToolResultRole, UserContent, UserMessage,
    UserRole,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::os::unix::fs::PermissionsExt;

async fn setup() -> (tempfile::TempDir, ProviderConfig) {
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("claude");
    std::fs::write(&executable, include_str!("fixtures/claude.py")).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir(dir.path().join(".pi")).unwrap();
    std::fs::write(dir.path().join(".pi/claude-bridge.json"), serde_json::to_string(&serde_json::json!({"startupNoticeShown":"2026-10-25", "provider":{"pathToClaudeCodeExecutable":executable}, "askClaude":{"enabled":true}})).unwrap()).unwrap();
    let mut registry = ExtensionRegistry::new();
    register(&mut registry);
    let loaded = registry
        .load_extensions(
            &["<native:pi-claude-bridge>".into()],
            &JsString::from(dir.path().to_str().unwrap()),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
    let pending = loaded.runtime.pending_provider_registrations();
    let config = pending
        .iter()
        .find(|p| p.name == "claude-bridge")
        .expect("native bridge provider")
        .config
        .clone();
    (dir, config)
}

fn context(prompt: &str) -> Context {
    Context {
        system_prompt: None,
        tools: None,
        messages: vec![Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(prompt.into()),
            input_source: None,
            provider_native: None,
            timestamp: 1.0,
        })],
    }
}

fn options(dir: &tempfile::TempDir) -> SimpleStreamOptions {
    let mut options = SimpleStreamOptions::default();
    options.stream.cwd = Some(dir.path().to_str().unwrap().into());
    options.stream.session_id = Some("pi-fixture-session".into());
    options
}

fn model(config: &ProviderConfig) -> pi_ai::types::Model {
    let mut model = pi_ai::providers::models_data::builtin_model("anthropic", "claude-opus-5-5").unwrap();
    model.api = config.api.clone().unwrap();
    model.provider = "claude-bridge".into();
    model.base_url = "claude-bridge".into();
    model
}

#[tokio::test]
async fn claude_bridge_fake_process_streams_lossless_text_and_usage() {
    let (dir, config) = setup().await;
    let stream = config.stream_simple.as_ref().unwrap()(model(&config), context("hello"), Some(options(&dir))).unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), stream.result())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop);
    let AssistantContent::Text(text) = &result.content[0] else {
        panic!("text");
    };
    assert_eq!(text.text, "Claude: hello");
    assert_eq!(result.usage.input, 13.0);
    assert_eq!(result.usage.cache_read, 40.0);
    assert_eq!(result.usage.output, 6.0);
}

#[tokio::test]
async fn claude_bridge_fake_process_tool_call_executes_pi_read_and_returns_its_result() {
    let (dir, config) = setup().await;
    std::fs::write(dir.path().join("fixture.txt"), "fixture from Pi read").unwrap();
    let cwd: JsString = dir.path().to_str().unwrap().into();
    let read = crate::core::tools::read::create_read_tool_definition(cwd, Default::default());
    let mut context = context("tool-round-trip");
    context.tools = Some(vec![Tool::new(
        "read",
        read.description.clone(),
        read.parameters.clone(),
    )]);
    let stream_fn = config.stream_simple.as_ref().unwrap();
    let stream = stream_fn(model(&config), context.clone(), Some(options(&dir))).unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), stream.result())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::ToolUse);
    let AssistantContent::ToolCall(call) = &result.content[0] else {
        panic!("tool call");
    };
    assert_eq!(call.name, "read");
    assert_eq!(
        call.arguments.get(&JsString::from("path")),
        Some(&JsValue::String("fixture.txt".into()))
    );
    let output = (read.execute)(
        call.id.clone(),
        JsValue::Object(call.arguments.clone()),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let content: Vec<pi_ai::types::ToolResultContent> =
        js_json::from_value(&js_json::to_value(&output.content).unwrap()).unwrap();
    context.messages.push(Message::Assistant(Box::new(result.clone())));
    context.messages.push(Message::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: call.id.clone(),
        tool_name: "read".into(),
        content,
        details: output.details,
        is_error: false,
        timestamp: 2.0,
    }));
    let stream = stream_fn(model(&config), context, Some(options(&dir))).unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), stream.result())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop);
    let AssistantContent::Text(text) = &result.content[0] else {
        panic!("text");
    };
    assert!(text.text.contains("fixture from Pi read"));
}
