use super::*;
use crate::core::extensions::{ExtensionRegistry, types::ProviderConfig};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{
    AssistantContent, Context, Message, StopReason, Tool, ToolResultMessage, ToolResultRole, UserContent, UserMessage,
    UserRole,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::os::unix::fs::PermissionsExt;

pub(super) async fn setup() -> (
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
    register_with_ownership(&mut registry, Arc::new(ownership::Ownership::default()));
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

pub(super) fn model(config: &ProviderConfig) -> pi_ai::types::Model {
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
async fn claude_bridge_one_off_summary_uses_raw_system_prompt_and_does_not_touch_session_mirrors() {
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    *bridge.sessions.lock().unwrap() = Ok(sessions::SessionStore::at(
        dir.path().join("cc-summary"),
        dir.path().join("agent-summary"),
    ));
    let mut ctx = context("part one");
    let mut first = JsString::from("part one ");
    first.push_unit(0xdfff);
    let Message::User(user) = &mut ctx.messages[0] else {
        panic!("user");
    };
    user.content = UserContent::Text(first.clone());
    ctx.messages.extend(context("part two").messages);
    let mut system = JsString::from("summary system ");
    system.push_unit(0xd800);
    ctx.system_prompt = Some(system);
    let mut opts = options(&dir);
    opts.stream.cache_retention = Some(pi_ai::types::CacheRetention::None);
    let mut events = bridge.stream(model(&config), ctx, Some(opts)).unwrap();
    let mut count = 0;
    while let Some(event) = events.next().await {
        assert!(matches!(
            event.unwrap(),
            pi_ai::types::AssistantMessageEvent::Done { .. }
        ));
        count += 1;
    }
    let result = events.result().await.unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop, "{:?}", result.error_message);
    let AssistantContent::Text(text) = &result.content[0] else {
        panic!("summary text");
    };
    assert_eq!(text.text, JsString::from("Claude: ") + first + "\npart two");
    assert_eq!(result.usage.total_tokens, 0.0);
    assert_eq!(count, 1);
    assert!(!dir.path().join("agent-summary").exists());
    assert!(!dir.path().join("cc-summary").exists());
}

#[tokio::test]
async fn claude_bridge_short_unowned_caller_preserves_the_longer_native_session() {
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    *bridge.sessions.lock().unwrap() = Ok(sessions::SessionStore::at(
        dir.path().join("cc-unowned"),
        dir.path().join("agent-unowned"),
    ));
    let mut opts = options(&dir);
    opts.stream.session_id = None;
    let mut history = context("hello");
    let first = bridge
        .stream(model(&config), history.clone(), Some(opts.clone()))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(first.stop_reason, StopReason::Stop);
    history.messages.push(Message::Assistant(Box::new(first)));
    let script = include_str!("fixtures/claude.py");
    std::fs::write(
        dir.path().join("claude"),
        script.replace(
            "11111111-1111-4111-8111-111111111111",
            "22222222-2222-4222-8222-222222222222",
        ),
    )
    .unwrap();
    let short = bridge
        .stream(model(&config), context("short-unowned"), Some(opts.clone()))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(short.stop_reason, StopReason::Stop, "{:?}", short.error_message);
    std::fs::write(dir.path().join("claude"), script).unwrap();
    history.messages.extend(context("assert-preserved-native").messages);
    let resumed = bridge
        .stream(model(&config), history, Some(opts))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(resumed.stop_reason, StopReason::Stop, "{:?}", resumed.error_message);
}

#[tokio::test]
async fn claude_bridge_orphaned_tool_results_finish_without_launching_a_new_query() {
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(
        JsValue::Null,
        Err(pi_js_compat::callback_error::CallbackError::new("must not launch")),
        "AskClaude".into(),
    );
    let context: Context = js_json::from_value(&js_json::parse_lossless(r#"{"messages":[{"role":"toolResult","toolCallId":"gone","toolName":"read","content":[{"type":"text","text":"orphaned"}],"isError":false,"timestamp":1}]}"#).unwrap()).unwrap();
    let result = bridge
        .stream(model(&config), context, Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop);
    assert!(result.content.is_empty());
}

#[tokio::test]
async fn claude_bridge_cancellation_during_initialize_ends_the_stream_and_reaps_the_child() {
    let (dir, config, _) = setup().await;
    std::fs::write(dir.path().join(".fixture-stall-initialize"), "").unwrap();
    let controller = pi_js_compat::abort::AbortController::new();
    let mut opts = options(&dir);
    opts.stream.request.signal = Some(controller.signal());
    let stream = config.stream_simple.as_ref().unwrap()(model(&config), context("wait"), Some(opts)).unwrap();
    let pid: u32 = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(dir.path().join(".fixture-pid"))
                && let Ok(pid) = text.parse()
            {
                break pid;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    controller.abort(None);
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), stream.result())
        .await
        .expect("abort must cover initialize")
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Aborted);
    assert_eq!(result.error_message.unwrap(), "Operation aborted");
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("Claude child must exit and be reaped");
}

fn read_context(prompt: &str) -> Context {
    let mut ctx = context(prompt);
    ctx.tools = Some(vec![Tool::new(
        "read",
        "Read a fixture",
        serde_json::json!({"type":"object","properties":{"path":{"type":"string"}}}),
    )]);
    ctx
}
fn append_read_result(ctx: &mut Context, assistant: pi_ai::types::AssistantMessage, text: JsString) {
    ctx.messages.push(Message::Assistant(Box::new(assistant)));
    ctx.messages.push(Message::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "tool-read".into(),
        tool_name: "read".into(),
        content: vec![pi_ai::types::ToolResultContent::Text(pi_ai::types::TextContent::new(
            text,
        ))],
        details: None,
        is_error: false,
        timestamp: 2.0,
    }));
}

#[tokio::test]
async fn claude_bridge_rewrite_discards_a_parked_query_and_reimports_authoritative_history() {
    let (dir, config, _) = setup().await;
    let ownership = Arc::new(ownership::Ownership::default());
    let bridge = provider::BridgeProvider::with_ownership(
        JsValue::Null,
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
        ownership.clone(),
    );
    let sponsor = provider::BridgeProvider::with_ownership(
        JsValue::Null,
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
        ownership,
    );
    *bridge.sessions.lock().unwrap() = Ok(sessions::SessionStore::at(
        dir.path().join("cc-rewrite"),
        dir.path().join("agent-rewrite"),
    ));
    let opts = options(&dir);
    let mut ctx = read_context("tool-round-trip");
    let first = bridge
        .stream(model(&config), ctx.clone(), Some(opts.clone()))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(first.stop_reason, StopReason::ToolUse);
    append_read_result(&mut ctx, first, "stale result".into());
    let Message::User(user) = &mut ctx.messages[0] else {
        panic!("user");
    };
    user.content = UserContent::Text("compacted history".into());
    sponsor
        .mark_rebuild(opts.stream.session_id.as_ref().unwrap(), false)
        .unwrap();
    let output = bridge
        .stream(model(&config), ctx, Some(opts))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::Stop, "{:?}", output.error_message);
    let AssistantContent::Text(text) = &output.content[0] else {
        panic!("continuation text");
    };
    assert_eq!(text.text, "rewritten continuation");
}

#[tokio::test]
async fn claude_bridge_steers_all_trailing_user_messages_before_releasing_tool_results() {
    let (dir, config, _) = setup().await;
    let stream = config.stream_simple.as_ref().unwrap();
    let mut ctx = read_context("steer-round-trip");
    let first = stream(model(&config), ctx.clone(), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(first.stop_reason, StopReason::ToolUse);
    append_read_result(&mut ctx, first, "steered result".into());
    ctx.messages.extend(context("steer one").messages);
    ctx.messages.extend(context("steer two").messages);
    let output = stream(model(&config), ctx, Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::Stop, "{:?}", output.error_message);
    let AssistantContent::Text(text) = &output.content[0] else {
        panic!("tool result text");
    };
    assert_eq!(text.text, "steered result");
}

#[tokio::test]
async fn claude_bridge_concurrent_sessions_do_not_cross_route_identical_tool_ids() {
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    let mut a = options(&dir);
    let mut b = options(&dir);
    a.stream.session_id = Some("session-a".into());
    b.stream.session_id = Some("session-b".into());
    let mut context_a = read_context("tool-round-trip");
    let mut context_b = context_a.clone();
    let first_a = bridge
        .stream(model(&config), context_a.clone(), Some(a.clone()))
        .unwrap();
    let first_b = bridge
        .stream(model(&config), context_b.clone(), Some(b.clone()))
        .unwrap();
    let (first_a, first_b) = tokio::join!(first_a.result(), first_b.result());
    append_read_result(&mut context_a, first_a.unwrap(), "result a".into());
    append_read_result(&mut context_b, first_b.unwrap(), "result b".into());
    let result_a = bridge.stream(model(&config), context_a, Some(a)).unwrap();
    let result_b = bridge.stream(model(&config), context_b, Some(b)).unwrap();
    let (result_a, result_b) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(result_a.result(), result_b.result())
    })
    .await
    .unwrap();
    for (result, expected) in [(result_a.unwrap(), "result a"), (result_b.unwrap(), "result b")] {
        assert_eq!(result.stop_reason, StopReason::Stop);
        let AssistantContent::Text(text) = &result.content[0] else {
            panic!("text");
        };
        assert_eq!(text.text, expected);
    }
}

#[tokio::test]
async fn claude_bridge_rate_limit_failures_are_labeled_without_losing_utf16() {
    use crate::core::extensions::types::{ExtensionUiContext, NotifyType};
    #[derive(Default)]
    struct Ui(std::sync::Mutex<Vec<(JsString, Option<NotifyType>)>>);
    impl ExtensionUiContext for Ui {
        fn notify(
            &self,
            text: &JsString,
            kind: Option<NotifyType>,
        ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
            self.0.lock().unwrap().push((text.clone(), kind));
            Ok(())
        }
    }
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    let ui = Arc::new(Ui::default());
    *bridge.ui.lock().unwrap() = Some(ui.clone());
    let output = bridge
        .stream(model(&config), context("rate-limit"), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::Error);
    let expected = js_json::parse_lossless(r#""Claude rate limit (five_hour): capacity \udfff""#).unwrap();
    assert_eq!(output.error_message.as_ref(), expected.as_string());
    let actual = ui.0.lock().unwrap();
    assert_eq!(
        actual.iter().map(|(text, _)| text.clone()).collect::<Vec<_>>(),
        vec![
            JsString::from("Claude rate limit warning: 81% used (five_hour)"),
            "Claude rate limit warning: 86% used (five_hour)".into(),
            "Claude rate limit warning: 81% used (five_hour)".into(),
            "Claude rate limited (five_hour) — resets at unknown".into(),
        ]
    );
    assert!(actual.iter().all(|(_, kind)| *kind == Some(NotifyType::Warning)));
}

#[tokio::test]
async fn claude_bridge_fake_process_writes_source_debug_cli_shape_and_diagnostic_formats() {
    let (dir, config, _) = setup().await;
    let mut bridge = provider::BridgeProvider::new(
        js_json::parse_lossless(r#"{"shapeTrace":true}"#).unwrap(),
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
    );
    bridge.logs.as_mut().unwrap().enabled = true;
    let mut options = options(&dir);
    options.stream.session_id = Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into());
    let output = bridge
        .stream(model(&config), context("hello"), Some(options))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::Stop);
    let logs = bridge.logs.as_ref().unwrap();
    let path = super::logging::debug_path().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(&format!("[{}] provider: fresh query", logs.module_id)));
    assert!(text.contains("[cli-stderr provider#1] fixture debug line"));
    let cli = std::fs::read_dir(path.parent().unwrap().join("cc-cli-logs"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .any(|p| std::fs::read_to_string(p).unwrap() == "fixture CLI debug log\n");
    assert!(cli);
    let shapes =
        std::fs::read_to_string(super::logging::agent_dir().unwrap().join("claude-bridge-shapes.jsonl")).unwrap();
    let shapes = shapes
        .lines()
        .map(|line| js_json::parse_lossless(line).unwrap())
        .collect::<Vec<_>>();
    assert!(shapes.iter().any(|v| {
        v.get("phase")
            .and_then(JsValue::as_string)
            .is_some_and(|s| s == "pi_final")
    }));
    let mut data = pi_js_compat::js_json::JsObject::new();
    data.insert("message".into(), JsValue::String(JsString::from_utf16(vec![0xdfff])));
    logs.diag("fixture_diagnostic", data).unwrap();
    let diagnostics =
        std::fs::read_to_string(super::logging::agent_dir().unwrap().join("claude-bridge-diag.log")).unwrap();
    let entry = diagnostics
        .lines()
        .map(|line| js_json::parse_lossless(line).unwrap())
        .find(|v| {
            v.get("label")
                .and_then(JsValue::as_string)
                .is_some_and(|s| s == "fixture_diagnostic")
        })
        .unwrap();
    assert_eq!(
        entry.get("moduleInstanceId").unwrap().as_string().unwrap(),
        logs.module_id.as_str()
    );
    assert_eq!(entry.get("message").unwrap().as_string().unwrap().units(), &[0xdfff]);
}

#[tokio::test]
async fn claude_bridge_auth_stderr_is_observed_before_a_running_query_exits() {
    let (dir, config, _) = setup().await;
    let home = dir.path().join("stderr-home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(
        home.join(".claude/live-stderr.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();
    let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    *bridge.accounts.lock().unwrap() = Ok(Some(
        account_state::AccountManager::new(home, vec!["live-stderr".into()]).unwrap(),
    ));
    let controller = pi_js_compat::abort::AbortController::new();
    let mut options = options(&dir);
    options.stream.request.signal = Some(controller.signal());
    let stream = bridge
        .stream(model(&config), context("auth-stderr-wait"), Some(options))
        .unwrap();
    let log = super::logging::agent_dir().unwrap().join("claude-bridge-auth.jsonl");
    let observed = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let found = std::fs::read_to_string(&log).ok().is_some_and(|text| {
                text.lines()
                    .filter_map(|line| js_json::parse_lossless(line).ok())
                    .any(|row| {
                        row.get("profile")
                            .and_then(JsValue::as_string)
                            .is_some_and(|s| s == "live-stderr")
                            && row
                                .get("event")
                                .and_then(JsValue::as_string)
                                .is_some_and(|s| s == "oauth_failure")
                    })
            });
            if found {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await;
    controller.abort(None);
    tokio::time::timeout(std::time::Duration::from_secs(2), stream.result())
        .await
        .unwrap()
        .unwrap();
    assert!(
        observed.is_ok(),
        "auth failure must be visible while the child is still running"
    );
}

#[tokio::test]
async fn claude_bridge_shape_trace_records_discarded_attempt_under_its_original_message_id() {
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(
        js_json::parse_lossless(r#"{"shapeTrace":true}"#).unwrap(),
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
    );
    let output = bridge
        .stream(model(&config), context("restream"), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::Stop);
    let text = std::fs::read_to_string(logging::agent_dir().unwrap().join("claude-bridge-shapes.jsonl")).unwrap();
    let discarded = text
        .lines()
        .map(|s| js_json::parse_lossless(s).unwrap())
        .find(|v| {
            v.get("phase")
                .and_then(JsValue::as_string)
                .is_some_and(|s| s == "discarded")
        })
        .expect("discarded phase");
    assert_eq!(
        discarded.get("sdkMessageId").and_then(JsValue::as_string).unwrap(),
        "msg_old"
    );
    assert_eq!(
        discarded.get("blockTypes").unwrap(),
        &js_json::parse_lossless(r#"["text"]"#).unwrap()
    );
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

#[tokio::test]
async fn claude_bridge_empty_results_emit_source_events_content_and_usage() {
    for (prompt, tags, text_blocks, input, output, error) in [
        (
            "empty-assistant-result",
            vec!["start", "text_start", "text_delta", "text_end", "done"],
            1,
            7.0,
            3.0,
            None,
        ),
        (
            "missing-content-result",
            vec!["start", "text_start", "text_delta", "text_end", "done"],
            1,
            0.0,
            0.0,
            None,
        ),
        ("empty-stream-result", vec!["start", "done"], 0, 7.0, 3.0, None),
        (
            "empty-stream-text",
            vec!["start", "text_start", "text_delta", "text_end", "done"],
            1,
            13.0,
            6.0,
            None,
        ),
        (
            "empty-is-error-result",
            vec!["start", "error"],
            0,
            0.0,
            0.0,
            Some(JsString::from("Claude Code reported an error")),
        ),
        (
            "text-is-error-result",
            vec!["start", "error"],
            0,
            0.0,
            0.0,
            Some(JsString::from("failed ") + JsString::from_utf16(vec![0xdfff])),
        ),
        (
            "empty-synthetic-error",
            vec!["start", "error"],
            0,
            7.0,
            0.0,
            Some(JsString::from("Claude Code reported an error")),
        ),
    ] {
        let (dir, config, _) = setup().await;
        let mut stream =
            config.stream_simple.as_ref().unwrap()(model(&config), context(prompt), Some(options(&dir))).unwrap();
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(js_json::to_value(&event.unwrap()).unwrap());
        }
        let actual: Vec<_> = events
            .iter()
            .map(|event| super::stream::string(event, "type"))
            .collect();
        assert_eq!(
            actual,
            tags.iter().map(|tag| JsString::from(*tag)).collect::<Vec<_>>(),
            "{prompt}"
        );
        let result = stream.result().await.unwrap();
        assert_eq!(result.content.len(), text_blocks, "{prompt}");
        for content in &result.content {
            let AssistantContent::Text(text) = content else {
                panic!("{prompt}: text");
            };
            assert_eq!(text.text, "", "{prompt}");
        }
        assert_eq!(result.usage.input, input, "{prompt}");
        assert_eq!(result.usage.output, output, "{prompt}");
        let cache_read = if prompt == "empty-stream-text" { 40.0 } else { 0.0 };
        assert_eq!(result.usage.total_tokens, input + output + cache_read, "{prompt}");
        assert_eq!(result.error_message, error, "{prompt}");
        assert_eq!(
            result.stop_reason,
            if error.is_some() {
                StopReason::Error
            } else {
                StopReason::Stop
            },
            "{prompt}"
        );
    }
}

#[tokio::test]
async fn claude_bridge_empty_tool_results_normalize_mcp_content_and_preserve_error() {
    for empty_block in [false, true] {
        let (dir, config, _) = setup().await;
        let stream = config.stream_simple.as_ref().unwrap();
        let mut ctx = read_context("empty-tool-result");
        let first = stream(model(&config), ctx.clone(), Some(options(&dir)))
            .unwrap()
            .result()
            .await
            .unwrap();
        assert_eq!(first.stop_reason, StopReason::ToolUse);
        assert_eq!(first.usage.total_tokens, 10.0);
        append_read_result(&mut ctx, first, "".into());
        let Some(Message::ToolResult(result)) = ctx.messages.last_mut() else {
            panic!("result");
        };
        result.is_error = true;
        if !empty_block {
            result.content.clear();
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            stream(model(&config), ctx, Some(options(&dir))).unwrap().result(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.stop_reason, StopReason::Stop, "{:?}", result.error_message);
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.usage.total_tokens, 0.0);
    }
}
