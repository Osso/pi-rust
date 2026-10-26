use super::*;
use crate::core::extensions::{ExtensionRegistry, types::ProviderConfig};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{
    AssistantContent, Context, Message, StopReason, Tool, ToolResultMessage, ToolResultRole, UserContent, UserMessage,
    UserRole,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::os::unix::fs::PermissionsExt;

async fn setup() -> (
    tempfile::TempDir,
    ProviderConfig,
    crate::core::extensions::types::LoadExtensionsResult,
) {
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
    (dir, config, loaded)
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
async fn claude_bridge_rejects_duplicate_account_profiles_before_launch() {
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(
        js_json::parse_lossless(r#"{"accountProfiles":["work","work"]}"#).unwrap(),
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
    );
    let result = bridge
        .stream(model(&config), context("hello"), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(result.error_message.unwrap(), "Duplicate profile name");
}

#[tokio::test]
async fn claude_bridge_profile_queries_writeback_and_share_quota_selection() {
    let (dir, config, _) = setup().await;
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    for name in ["a", "b"] {
        std::fs::write(
            home.join(format!(".claude/{name}.credentials.json")),
            r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
        )
        .unwrap();
    }
    let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    *bridge.accounts.lock().unwrap() = Ok(Some(
        account_state::AccountManager::new(home.clone(), vec!["a".into(), "b".into()]).unwrap(),
    ));
    let first = bridge
        .stream(model(&config), context("account-quota"), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(first.stop_reason, StopReason::Error);
    let second = bridge
        .stream(model(&config), context("account-quota"), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(second.stop_reason, StopReason::Stop);
    assert_eq!(
        account_profiles::read_usage_credentials(&home, Some("b"), None)
            .unwrap()
            .get("claudeAiOauth")
            .unwrap()
            .get("accessToken")
            .unwrap()
            .as_string()
            .unwrap(),
        "refreshed"
    );
    let mut peer = account_state::AccountManager::new(home, vec!["a".into(), "b".into()]).unwrap();
    assert_eq!(peer.selected_profile().unwrap().as_deref(), Some("b"));
}

#[tokio::test]
async fn claude_bridge_imports_prior_history_before_resuming_native_session() {
    let (dir, config, _) = setup().await;
    let stream_fn = config.stream_simple.as_ref().unwrap();
    let prior: JsString = JsString::from("earlier ") + JsString::from_utf16(vec![0xdfff]);
    let mut history = context("");
    let Message::User(user) = &mut history.messages[0] else {
        panic!("user")
    };
    user.content = UserContent::Text(prior);
    let result = stream_fn(model(&config), history.clone(), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    history.messages.push(Message::Assistant(Box::new(result)));
    history.messages.extend(context("assert-history").messages);
    let bridge = provider::BridgeProvider::new(
        JsValue::object([] as [(&str, JsValue); 0]),
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
    );
    *bridge.sessions.lock().unwrap() = Ok(sessions::SessionStore::at(
        dir.path().join("claude-data"),
        dir.path().join("agent"),
    ));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        bridge
            .stream(model(&config), history, Some(options(&dir)))
            .unwrap()
            .result(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop, "{:?}", result.error_message);
    let AssistantContent::Text(text) = &result.content[0] else {
        panic!("text")
    };
    assert_eq!(text.text, "history restored");
}

#[tokio::test]
async fn claude_bridge_fake_process_streams_lossless_text_and_usage() {
    let (dir, config, _) = setup().await;
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
    let (dir, config, _) = setup().await;
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

#[tokio::test]
async fn claude_bridge_fake_process_askclaude_uses_registered_tool() {
    use crate::core::auth_storage::AuthStorage;
    use crate::core::extensions::ExtensionRunner;
    use crate::core::model_registry::ModelRegistry;
    use crate::core::session_manager::SessionManager;
    use std::sync::{Arc, Mutex};
    let (dir, _, loaded) = setup().await;
    let definition = loaded.extensions[0]
        .read()
        .tools
        .get("AskClaude")
        .expect("opt-in AskClaude registration")
        .definition
        .clone();
    let cwd: JsString = dir.path().to_str().unwrap().into();
    let sessions = Arc::new(Mutex::new(SessionManager::in_memory(Some(&cwd), None).unwrap()));
    let models = Arc::new(ModelRegistry::in_memory(AuthStorage::in_memory(Vec::<(JsString, JsValue)>::new())).unwrap());
    let runner = ExtensionRunner::new(loaded.extensions, loaded.runtime, cwd, sessions, models, None);
    let args = js_json::parse_lossless(r#"{"prompt":"explain this","isolated":true,"mode":"none"}"#).unwrap();
    let output = (definition.execute)("ask1".into(), args, None, None, Some(runner.create_context()))
        .await
        .unwrap();
    let pi_ai::types::TextOrImageContent::Text(text) = &output.content[0] else {
        panic!("text");
    };
    assert_eq!(text.text, "Claude: explain this");
    assert_eq!(
        output
            .details
            .as_ref()
            .and_then(|v| v.get("prompt"))
            .and_then(JsValue::as_string),
        Some(&JsString::from("explain this"))
    );
    assert_ne!(
        output
            .details
            .as_ref()
            .and_then(|v| v.get("error"))
            .and_then(JsValue::as_bool),
        Some(true)
    );
}
