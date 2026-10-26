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
    options.stream.session_id = Some(
        format!(
            "pi-fixture-session-{}",
            dir.path().file_name().unwrap().to_str().unwrap()
        )
        .into(),
    );
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
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
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
    assert_eq!(text.text, JsString::from("Claude: ") + first.clone() + "\npart two");
    assert_eq!(
        pi_js_compat::js_json::parse_lossless(
            std::fs::read_to_string(dir.path().join(".fixture-first-user.json")).unwrap()
        )
        .unwrap(),
        super::sdk_launch_tests::sdk_string_prompt(first + "\npart two")
    );
    assert_eq!(result.usage.total_tokens, 0.0);
    assert_eq!(count, 1);
    assert!(!dir.path().join("agent-summary").exists());
    assert!(!dir.path().join("cc-summary").exists());
    let text = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    let module = &bridge.logs.as_ref().unwrap().module_id;
    for line in [
        "provider: one-off summarizer call (cacheRetention none) routed to isolated summary, msgs=2",
        "compact summary: spawn model=claude-opus-5-5[1m] registeredModel=claude-opus-5-5 promptLen=19",
        "compact summary: first event type=system",
        "compact summary: done textLen=27",
    ] {
        assert!(text.contains(&format!("[{module}] {line}\n")), "missing {line}");
    }
    let mut opts = options(&dir);
    opts.stream.cache_retention = Some(pi_ai::types::CacheRetention::None);
    let failed = bridge
        .stream(model(&config), context("summary-error"), Some(opts))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(failed.stop_reason, StopReason::Error);
    assert_eq!(
        failed.error_message.unwrap(),
        JsString::from("summary failed ") + JsString::from_utf16(vec![0xdfff])
    );
    let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    assert!(log.contains(&format!("[{module}] compact summary: error summary failed �\n")));
    assert!(!log.contains(&format!("[{module}] runIsolatedSummary threw; pushing terminal error")));
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
        .mark_rebuild(opts.stream.session_id.as_ref().unwrap(), false, "session_compact")
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
async fn claude_bridge_debug_primary_query_is_not_a_remaining_reentrant_query() {
    let (dir, config, _) = setup().await;
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
    let mut a = options(&dir);
    a.stream.session_id = Some("log-primary".into());
    let mut b = options(&dir);
    b.stream.session_id = Some("log-reentrant".into());
    let mut ctx_a = read_context("tool-round-trip");
    let first_a = bridge
        .stream(model(&config), ctx_a.clone(), Some(a.clone()))
        .unwrap()
        .result()
        .await
        .unwrap();
    let first_b = bridge
        .stream(model(&config), read_context("tool-round-trip"), Some(b.clone()))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(first_b.stop_reason, StopReason::ToolUse);
    append_read_result(&mut ctx_a, first_a, "primary done".into());
    let done = bridge
        .stream(model(&config), ctx_a, Some(a))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(done.stop_reason, StopReason::Stop);
    bridge.captures = Arc::new(std::sync::Mutex::new(prompt_capture::PromptCaptures::default()));
    let mut unknown = context("unresolved capture");
    unknown.system_prompt = Some("no captured prompt".into());
    let diagnostic = bridge
        .stream(model(&config), unknown, Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(diagnostic.stop_reason, StopReason::Error);
    let fresh = bridge
        .stream(model(&config), context("hello"), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(fresh.stop_reason, StopReason::Stop);
    bridge
        .shutdown(b.stream.session_id.as_ref().unwrap(), &"cleanup".into())
        .await
        .unwrap();
    let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    let module = &bridge.logs.as_ref().unwrap().module_id;
    let entries = log
        .lines()
        .filter_map(|line| line.split_once(&format!("[{module}] ")).map(|(_, text)| text))
        .filter(|line| line.starts_with("provider: streamClaudeAgentSdk called"))
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        vec![
            "provider: streamClaudeAgentSdk called, activeQuery=false, lastMsgRole=user, isReentrant=false",
            "provider: streamClaudeAgentSdk called, activeQuery=true, lastMsgRole=user, isReentrant=true",
            "provider: streamClaudeAgentSdk called, activeQuery=true, lastMsgRole=toolResult, isReentrant=true",
            "provider: streamClaudeAgentSdk called, activeQuery=false, lastMsgRole=user, isReentrant=false",
            "provider: streamClaudeAgentSdk called, activeQuery=false, lastMsgRole=user, isReentrant=false",
        ]
    );
    let diagnostics = std::fs::read_to_string(logging::agent_dir().unwrap().join("claude-bridge-diag.log")).unwrap();
    let row = diagnostics
        .lines()
        .map(|line| js_json::parse_lossless(line).unwrap())
        .find(|row| {
            stream::string(row, "moduleInstanceId") == module.as_str()
                && stream::string(row, "label") == "prompt_capture_unresolved"
        })
        .unwrap();
    assert_eq!(row.get("reentrantUserQuery"), Some(&JsValue::Bool(false)));
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
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
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
    let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    assert!(log.contains(&format!(
        "[{}] consumeQuery: error result, subtype=success, error=Claude rate limit (five_hour): capacity �\n",
        bridge.logs.as_ref().unwrap().module_id
    )));
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
    // index.ts:109 splits each stderr chunk on /\r?\n/ only; a bare CR stays inside the record.
    assert!(
        text.contains("[cli-stderr provider#1] fixture debug\rline\n"),
        "{text:?}"
    );
    assert!(text.contains("[cli-stderr provider#1] second line\n"), "{text:?}");
    let messages = text
        .lines()
        .filter_map(|line| {
            line.split_once(&format!("[{}] ", logs.module_id))
                .map(|(_, message)| message)
        })
        .collect::<Vec<_>>();
    assert!(
        messages
            .contains(&"provider: streamClaudeAgentSdk called, activeQuery=false, lastMsgRole=user, isReentrant=false")
    );
    assert!(messages.contains(&"consumeQuery: for-await loop exited, wasAborted=false, capturedSessionId=11111111"));
    assert!(messages.contains(&"provider: clearing activeQuery before final stream completion"));
    assert!(messages.contains(&"Case 1: clean start, 1 total messages"));
    assert!(messages.contains(&"syncResult: path=clean-start"));
    assert!(messages.contains(&"provider: fresh query model=claude-opus-5-5[1m] msgs=1 tools=0 resume=none effort=default ctxFiles=0 strictMcp=true prompt=hello"));
    assert!(messages.contains(&"provider: consumeQuery completed, stopReason=stop, error=undefined, aborted=false"));
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
async fn claude_bridge_primary_shape_reset_after_completed_query_uses_previous_trace() {
    assert_primary_shape_reset(false).await;
}

#[tokio::test]
async fn claude_bridge_primary_shape_reset_before_parked_replacement_uses_active_snapshot() {
    assert_primary_shape_reset(true).await;
}

async fn assert_primary_shape_reset(parked: bool) {
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(
        js_json::parse_lossless(r#"{"shapeTrace":true}"#).unwrap(),
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
    );
    *bridge.sessions.lock().unwrap() = Ok(sessions::SessionStore::at(
        dir.path().join("cc-shape"),
        dir.path().join("agent-shape"),
    ));
    let mut opts = options(&dir);
    opts.stream.session_id = Some(
        if parked {
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd"
        } else {
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc"
        }
        .into(),
    );
    let key = opts.stream.session_id.clone().unwrap();
    let first = bridge
        .stream(
            model(&config),
            if parked {
                read_context("shape-parked")
            } else {
                context("restream")
            },
            Some(opts.clone()),
        )
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(
        first.stop_reason,
        if parked { StopReason::ToolUse } else { StopReason::Stop }
    );
    let old_pid = std::fs::read_to_string(dir.path().join(".fixture-pid")).unwrap();
    let path = logging::agent_dir().unwrap().join("claude-bridge-shapes.jsonl");
    let before = std::fs::read_to_string(&path).unwrap();
    let previous = before
        .lines()
        .map(|line| js_json::parse_lossless(line).unwrap())
        .find(|row| {
            stream::string(row, "piSessionId") == key
                && stream::string(row, "phase") == if parked { "pi_tool_boundary" } else { "pi_final" }
        })
        .unwrap();
    let mut next = if parked {
        read_context("compacted history")
    } else {
        context("hello")
    };
    if parked {
        bridge.mark_rebuild(&key, false, "session_compact").unwrap();
        next.messages.push(Message::Assistant(Box::new(first)));
        next.messages.push(Message::ToolResult(ToolResultMessage {
            role: ToolResultRole::Tag,
            tool_call_id: "toolu_fallback".into(),
            tool_name: "read".into(),
            content: vec![pi_ai::types::ToolResultContent::Text(pi_ai::types::TextContent::new(
                "old result",
            ))],
            details: None,
            is_error: false,
            timestamp: 2.0,
        }));
    }
    let output = bridge
        .stream(model(&config), next, Some(opts))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::Stop, "{:?}", output.error_message);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while std::path::Path::new(&format!("/proc/{old_pid}")).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    let (line, row) = text
        .lines()
        .find_map(|line| {
            let row = js_json::parse_lossless(line).unwrap();
            (row.get("traceId") == previous.get("traceId") && stream::string(&row, "phase") == "turn_reset")
                .then_some((line, row))
        })
        .expect("turn_reset appended to previous primary trace");
    let mut expected = pi_js_compat::js_json::JsObject::new();
    expected.insert("timestamp".into(), row.get("timestamp").unwrap().clone());
    expected.insert("pid".into(), JsValue::Number(std::process::id().into()));
    expected.insert("traceId".into(), previous.get("traceId").unwrap().clone());
    expected.insert("piSessionId".into(), JsValue::String(key));
    expected.insert("source".into(), JsValue::String("pi".into()));
    expected.insert("phase".into(), JsValue::String("turn_reset".into()));
    expected.insert(
        "sdkMessageId".into(),
        JsValue::String(if parked { "msg_parked" } else { "msg_new" }.into()),
    );
    expected.insert(
        "blockTypes".into(),
        js_json::parse_lossless(if parked { r#"["toolCall"]"# } else { r#"["text"]"# }).unwrap(),
    );
    expected.insert(
        "toolCallIds".into(),
        js_json::parse_lossless(if parked { r#"["toolu_fallback"]"# } else { "[]" }).unwrap(),
    );
    assert_eq!(line, js_json::stringify_lossless(&JsValue::Object(expected)));
}

#[tokio::test]
async fn claude_bridge_fallback_tool_boundary_shape_keeps_the_assistant_message_id() {
    let (dir, config, _) = setup().await;
    let bridge = provider::BridgeProvider::new(
        js_json::parse_lossless(r#"{"shapeTrace":true}"#).unwrap(),
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
    );
    let mut opts = options(&dir);
    opts.stream.session_id = Some("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into());
    let key = opts.stream.session_id.clone().unwrap();
    let mut ctx = read_context("shape-fallback");
    let output = bridge
        .stream(model(&config), ctx.clone(), Some(opts))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::ToolUse);
    ctx.messages.push(Message::Assistant(Box::new(output)));
    ctx.messages.push(Message::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "toolu_fallback".into(),
        tool_name: "read".into(),
        content: vec![pi_ai::types::ToolResultContent::Text(pi_ai::types::TextContent::new(
            "shape done",
        ))],
        details: None,
        is_error: false,
        timestamp: 2.0,
    }));
    bridge.end_turn(&key, ctx).await.unwrap();
    let text = std::fs::read_to_string(logging::agent_dir().unwrap().join("claude-bridge-shapes.jsonl")).unwrap();
    let row = text
        .lines()
        .map(|line| js_json::parse_lossless(line).unwrap())
        .find(|row| stream::string(row, "piSessionId") == key && stream::string(row, "phase") == "pi_tool_boundary")
        .unwrap();
    assert_eq!(row.get("sdkMessageId"), Some(&JsValue::String("msg_fallback".into())));
    assert!(
        !text
            .lines()
            .map(|line| js_json::parse_lossless(line).unwrap())
            .any(|row| stream::string(&row, "piSessionId") == key && stream::string(&row, "phase") == "pi_final")
    );
}

#[tokio::test]
async fn claude_bridge_prompt_skips_invalid_images_and_uses_source_text_or_continue_fallback() {
    let (dir, config, _) = setup().await;
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
    for (raw, expected) in [
        (
            r#"[{"type":"text","text":"image-prompt"},{"type":"image","data":"","mimeType":"image/png"},{"type":"image","data":"AA==","mimeType":""},{"type":"image","data":"AQ==","mimeType":"image/png"}]"#,
            "image accepted",
        ),
        (
            r#"[{"type":"text","text":"text-fallback"},{"type":"image","data":"","mimeType":"image/png"}]"#,
            "text accepted",
        ),
        (r#"[{"type":"image","data":"AA==","mimeType":""}]"#, "continued"),
    ] {
        let mut ctx = context("");
        let Message::User(user) = &mut ctx.messages[0] else {
            panic!("user");
        };
        user.content = js_json::from_value(&js_json::parse_lossless(raw).unwrap()).unwrap();
        let output = bridge
            .stream(model(&config), ctx, Some(options(&dir)))
            .unwrap()
            .result()
            .await
            .unwrap();
        assert_eq!(output.stop_reason, StopReason::Stop, "{:?}", output.error_message);
        let AssistantContent::Text(text) = &output.content[0] else {
            panic!("text");
        };
        assert_eq!(text.text, expected);
    }
    let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    let module = &bridge.logs.as_ref().unwrap().module_id;
    let lines = log
        .lines()
        .filter_map(|line| line.split_once(&format!("[{module}] ")).map(|(_, text)| text))
        .filter(|line| line.starts_with("image block") || line.starts_with("extractUserPromptBlocks:"))
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec![
            "image block missing data or mimeType, skipping: keys=type,data,mimeType",
            "image block missing data or mimeType, skipping: keys=type,data,mimeType",
            "image block: mimeType=image/png, data length=4",
            "extractUserPromptBlocks: 1 msgs in turn, 2 blocks, types=text,image",
            "image block missing data or mimeType, skipping: keys=type,data,mimeType",
            "extractUserPromptBlocks: 1 msgs in turn, 1 blocks, types=text",
            "image block missing data or mimeType, skipping: keys=type,data,mimeType",
            "extractUserPromptBlocks: 1 msgs in turn, 0 blocks, types=",
        ]
    );
}

#[tokio::test]
async fn claude_bridge_missed_steers_preserve_results_mark_rebuild_and_log_source_outcomes() {
    for missing in [true, false] {
        let (dir, config, _) = setup().await;
        let mut bridge =
            provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
        bridge.logs.as_mut().unwrap().enabled = true;
        bridge.inject_steer_failure(if missing {
            None
        } else {
            Some("fixture steer rejected".into())
        });
        *bridge.sessions.lock().unwrap() = Ok(sessions::SessionStore::at(
            dir.path().join("cc"),
            dir.path().join("agent"),
        ));
        let mut ctx = read_context("tool-round-trip");
        let opts = options(&dir);
        let key = opts.stream.session_id.clone().unwrap();
        let first = bridge
            .stream(model(&config), ctx.clone(), Some(opts.clone()))
            .unwrap()
            .result()
            .await
            .unwrap();
        assert_eq!(first.stop_reason, StopReason::ToolUse);
        ctx.messages.push(Message::Assistant(Box::new(first)));
        let text = JsString::from("result kept ") + JsString::from_utf16(vec![0xdfff]);
        ctx.messages.push(Message::ToolResult(ToolResultMessage {
            role: ToolResultRole::Tag,
            tool_call_id: "tool-read".into(),
            tool_name: "read".into(),
            content: vec![pi_ai::types::ToolResultContent::Text(pi_ai::types::TextContent::new(
                text.clone(),
            ))],
            details: None,
            is_error: false,
            timestamp: 2.0,
        }));
        ctx.messages.extend(context("lost steer").messages);
        let output = bridge
            .stream(model(&config), ctx, Some(opts))
            .unwrap()
            .result()
            .await
            .unwrap();
        assert_eq!(output.stop_reason, StopReason::Stop, "{:?}", output.error_message);
        let AssistantContent::Text(answer) = &output.content[0] else {
            panic!("text");
        };
        assert_eq!(answer.text, text);
        let delivered = std::fs::read_to_string(dir.path().join(".fixture-tool-delivered")).unwrap();
        assert_eq!(js_json::parse_lossless(&delivered).unwrap(), JsValue::String(text));
        assert!(bridge.sessions.lock().unwrap().as_ref().unwrap().needs_rebuild(&key));
        let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
        let module = &bridge.logs.as_ref().unwrap().module_id;
        let expected = if missing {
            "WARNING: steer with no prompt stream, dropping: lost steer"
        } else {
            "provider: steer push rejected, delivering tool result anyway: Error: fixture steer rejected"
        };
        assert!(log.contains(&format!("[{module}] {expected}\n")));
        assert!(log.contains(&format!(
            "[{module}] provider: steer never reached CC, marked query for rebuild: lost steer\n"
        )));
    }
}

#[tokio::test]
async fn claude_bridge_shutdown_reports_exact_live_context_handler_and_prompt_counts() {
    use crate::core::extensions::types::{ExtensionEvent, SessionShutdownEvent, SessionShutdownReason};
    let (dir, config, _) = setup().await;
    let owner = Arc::new(ownership::Ownership::default());
    let mut bridge = provider::BridgeProvider::with_ownership(
        JsValue::Null,
        Ok(dir.path().join("claude")),
        "AskClaude".into(),
        owner.clone(),
    );
    assert!(owner.claim(&bridge.identity));
    bridge.logs.as_mut().unwrap().enabled = true;
    let mut registry = ExtensionRegistry::new();
    let fixture = bridge.clone();
    registry.register_hidden(
        "<fixture:leak-report>",
        Arc::new(move |pi| {
            let result = lifecycle::register(pi, fixture.clone());
            Box::pin(async { result })
        }),
    );
    let loaded = registry
        .load_extensions(
            &["<fixture:leak-report>".into()],
            &dir.path().to_str().unwrap().into(),
            None,
            None,
        )
        .await
        .unwrap();
    let runner = lifecycle_tests::runner(dir.path(), loaded, Some(model(&config)));
    let mut opts = options(&dir);
    opts.stream.session_id = Some(runner.create_context().session_manager().unwrap().get_session_id());
    let output = bridge
        .stream(model(&config), read_context("tool-round-trip"), Some(opts))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::ToolUse);
    let module = &bridge.logs.as_ref().unwrap().module_id;
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if std::fs::read_to_string(logging::debug_path().unwrap())
                .unwrap()
                .contains(&format!("[{module}] mcp handler: read [tool-read] → waiting\n"))
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    runner
        .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
            reason: SessionShutdownReason::Quit,
            target_session_file: None,
        }))
        .await;
    let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    assert!(log.contains(&format!(
        "[{module}] WARNING: session_shutdown left state behind — contexts=1 pendingToolCalls=1 promptStreams=1\n"
    )));
    runner
        .emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
            reason: SessionShutdownReason::Quit,
            target_session_file: None,
        }))
        .await;
    let after = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    assert!(after.contains(&format!("[{module}] session_shutdown: clearing 0 shared sessions\n")));
    assert_eq!(
        after
            .lines()
            .filter(|line| line.ends_with(&format!(
                "[{module}] session_shutdown: clearing ACTIVE_STREAM_SIMPLE_KEY"
            )))
            .count(),
        1
    );
    assert_eq!(
        after
            .lines()
            .filter(|line| line.contains(&format!("[{module}] WARNING: session_shutdown left state behind")))
            .count(),
        1
    );
    let pid = std::fs::read_to_string(dir.path().join(".fixture-pid")).unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
}

#[tokio::test]
async fn claude_bridge_empty_prompt_writes_unconditional_source_diagnostic() {
    for reentrant in [false, true] {
        let (dir, config, _) = setup().await;
        let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
        if reentrant {
            let mut primary = options(&dir);
            primary.stream.session_id = Some("empty-prompt-primary".into());
            let output = bridge
                .stream(model(&config), read_context("tool-round-trip"), Some(primary))
                .unwrap()
                .result()
                .await
                .unwrap();
            assert_eq!(output.stop_reason, StopReason::ToolUse);
        }
        let mut ctx = context("");
        ctx.messages.clear();
        let output = bridge
            .stream(model(&config), ctx, Some(options(&dir)))
            .unwrap()
            .result()
            .await
            .unwrap();
        if reentrant {
            bridge
                .shutdown(&"empty-prompt-primary".into(), &"cleanup".into())
                .await
                .unwrap();
        }
        assert_eq!(output.stop_reason, StopReason::Stop);
        let log = std::fs::read_to_string(logging::agent_dir().unwrap().join("claude-bridge-diag.log")).unwrap();
        let row = log
            .lines()
            .map(|line| js_json::parse_lossless(line).unwrap())
            .find(|row| {
                stream::string(row, "moduleInstanceId") == bridge.logs.as_ref().unwrap().module_id.as_str()
                    && stream::string(row, "label") == "empty_prompt"
            })
            .expect("unconditional empty-prompt diagnostic");
        let mut fields = row.as_object().unwrap().clone();
        fields.shift_remove(&JsString::from("ts"));
        fields.shift_remove(&JsString::from("moduleInstanceId"));
        let expected = JsValue::object([
            ("label", JsValue::String("empty_prompt".into())),
            ("contextLength", JsValue::Number(0.0)),
            ("isReentrant", JsValue::Bool(reentrant)),
            (
                "activeQueryContexts",
                JsValue::Number(if reentrant { 1.0 } else { 0.0 }),
            ),
            ("activeQueryExists", JsValue::Bool(false)),
            ("sharedSession", JsValue::Null),
            ("messageRoles", JsValue::String("".into())),
        ]);
        assert_eq!(
            js_json::stringify_lossless(&JsValue::Object(fields)),
            js_json::stringify_lossless(&expected)
        );
    }
}

#[tokio::test]
async fn claude_bridge_stream_debug_call_sites_keep_source_names_and_field_order() {
    let (dir, config, _) = setup().await;
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
    let output = bridge
        .stream(model(&config), context("logging-events"), Some(options(&dir)))
        .unwrap()
        .result()
        .await
        .unwrap();
    assert_eq!(output.stop_reason, StopReason::Stop);
    let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    let module = &bridge.logs.as_ref().unwrap().module_id;
    for expected in [
        "consumeQuery: unhandled SDK message type fixture_notice",
        "processStreamEvent: unhandled content_block_start type redacted_thinking",
        "processStreamEvent: skipping tool_use outside pi's MCP tools WebSearch [native-search] — handled internally by CC",
        "processStreamEvent: unhandled event type ping",
        "usage: in=13 out=6 cacheRead=40 cacheWrite=0 total=59 cachePct=75% model=claude-opus-5-5",
    ] {
        assert!(log.contains(&format!("[{module}] {expected}\n")), "missing {expected}");
    }
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
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
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
    let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    assert!(log.contains(&format!(
        "[{}] provider: abort detected, marked sharedSession needsRebuild + forceRotate\n",
        bridge.logs.as_ref().unwrap().module_id
    )));
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
async fn claude_bridge_cancellation_interrupts_a_blocked_control_reply_and_reaps_the_child() {
    assert_blocked_control_cancellation(false).await;
}

#[tokio::test]
async fn claude_bridge_shutdown_interrupts_a_blocked_control_reply_and_reaps_the_child() {
    assert_blocked_control_cancellation(true).await;
}

async fn assert_blocked_control_cancellation(shutdown: bool) {
    let (dir, config, loaded) = setup().await;
    let runner = super::lifecycle_tests::runner(dir.path(), loaded, Some(model(&config)));
    let controller = pi_js_compat::abort::AbortController::new();
    let mut opts = options(&dir);
    opts.stream.request.signal = Some(controller.signal());
    opts.stream.session_id = Some(runner.create_context().session_manager().unwrap().get_session_id());
    let mut ctx = read_context("blocked-control-write");
    ctx.tools.as_mut().unwrap()[0].description = "x".repeat(2 * 1024 * 1024).into();
    let stream = config.stream_simple.as_ref().unwrap()(model(&config), ctx, Some(opts)).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !dir.path().join(".fixture-control-blocked").exists() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    if shutdown {
        use crate::core::extensions::types::{ExtensionEvent, SessionShutdownEvent, SessionShutdownReason};
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            runner.emit(&mut ExtensionEvent::SessionShutdown(SessionShutdownEvent {
                reason: SessionShutdownReason::Quit,
                target_session_file: None,
            })),
        )
        .await
        .expect("shutdown interrupts control writes");
    } else {
        controller.abort(None);
    }
    let output = tokio::time::timeout(std::time::Duration::from_secs(1), stream.result())
        .await
        .expect("abort must interrupt a full stdin pipe")
        .unwrap();
    assert_eq!(
        output.stop_reason,
        if shutdown {
            StopReason::Error
        } else {
            StopReason::Aborted
        }
    );
    if shutdown {
        assert_eq!(output.error_message.unwrap(), "pi session shut down");
    }
    let pid = std::fs::read_to_string(dir.path().join(".fixture-pid")).unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
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
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
    let stream_fn = |model, context, options| bridge.stream(model, context, options);
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
    let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    let module = &bridge.logs.as_ref().unwrap().module_id;
    assert!(log.contains(&format!(
        "[{module}] provider: tool results, 1 results, 1 waiting handlers, ctx.msgs=3\n"
    )));
    assert!(log.contains(&format!("[{module}] mcp handler: read [tool-read] → waiting\n")));
}

/// index.ts:1852-1859 records only messages the SDK yields; it handles control
/// traffic internally. Re-runs the round trip in a child test process because
/// CLAUDE_BRIDGE_RECORD_STREAM is process-wide.
#[test]
fn claude_bridge_record_stream_appends_only_sdk_yielded_messages() {
    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("stream.jsonl");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_user_extensions::claude_bridge::integration_tests::claude_bridge_fake_process_tool_call_executes_pi_read_and_returns_its_result",
        ])
        .env("CLAUDE_BRIDGE_RECORD_STREAM", &record)
        .output()
        .unwrap();
    assert!(
        String::from_utf8(output.stdout.clone())
            .unwrap()
            .contains("running 1 test"),
        "{output:?}"
    );
    assert!(output.status.success(), "{output:?}");
    let types = std::fs::read_to_string(&record)
        .unwrap()
        .lines()
        .map(|line| {
            let frame = js_json::parse_lossless(line).unwrap();
            stream::string(&frame, "type").to_utf8().unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(types.first().map(String::as_str), Some("system"), "{types:?}");
    assert_eq!(types.last().map(String::as_str), Some("result"), "{types:?}");
    assert!(types.iter().any(|kind| kind == "stream_event"), "{types:?}");
    assert!(!types.iter().any(|kind| kind.starts_with("control_")), "{types:?}");
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

#[tokio::test]
async fn claude_bridge_askclaude_cancels_initialization_and_reaps_on_result_errors() {
    use std::sync::Arc;
    for stall in [true, false] {
        let (dir, _, loaded) = setup().await;
        if stall {
            std::fs::write(dir.path().join(".fixture-stall-initialize"), "").unwrap();
        }
        let definition = loaded.extensions[0]
            .read()
            .tools
            .get("AskClaude")
            .unwrap()
            .definition
            .clone();
        let runner = super::lifecycle_tests::runner(dir.path(), loaded, None);
        let controller = pi_js_compat::abort::AbortController::new();
        let args = JsValue::object([
            (
                "prompt",
                JsValue::String(if stall { "wait" } else { "error-wait" }.into()),
            ),
            ("isolated", JsValue::Bool(true)),
            ("mode", JsValue::String("none".into())),
        ]);
        let updates = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = updates.clone();
        let execute = tokio::spawn((definition.execute)(
            "ask-cancel".into(),
            args,
            Some(controller.signal()),
            Some(Arc::new(move |update| {
                captured.lock().unwrap().push(update);
                Ok(())
            })),
            Some(runner.create_context()),
        ));
        let pid = wait_for_fixture_pid(&dir).await;
        if stall {
            controller.abort(None);
        }
        let output = tokio::time::timeout(std::time::Duration::from_secs(1), execute)
            .await
            .expect("AskClaude cancellation/error must settle and reap")
            .unwrap()
            .unwrap();
        if !stall {
            assert_eq!(output.details.unwrap().get("error"), Some(&JsValue::Bool(true)));
            assert!(
                updates.lock().unwrap().is_empty(),
                "source error branch does not blank progress"
            );
        }
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "AskClaude must await cleanup: stalled={stall} pid={pid} status={:?}",
            std::fs::read_to_string(format!("/proc/{pid}/status"))
        );
    }
}

#[tokio::test]
async fn claude_bridge_summary_abort_reaps_its_child_before_terminal_output() {
    let (dir, config, _) = setup().await;
    std::fs::write(dir.path().join(".fixture-stall-initialize"), "").unwrap();
    for _ in 0..8 {
        let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
        let controller = pi_js_compat::abort::AbortController::new();
        let mut opts = options(&dir);
        opts.stream.cache_retention = Some(pi_ai::types::CacheRetention::None);
        opts.stream.request.signal = Some(controller.signal());
        let stream = bridge.stream(model(&config), context("summary"), Some(opts)).unwrap();
        let pid = wait_for_fixture_pid(&dir).await;
        controller.abort(None);
        assert_eq!(stream.result().await.unwrap().stop_reason, StopReason::Aborted);
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "summary must reap before delivering abort: pid={pid:?}"
        );
        std::fs::remove_file(dir.path().join(".fixture-pid")).unwrap();
    }
}

async fn wait_for_fixture_pid(dir: &tempfile::TempDir) -> String {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match std::fs::read_to_string(dir.path().join(".fixture-pid")) {
                Ok(pid) if pid.parse::<u32>().is_ok() => return pid,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("Cannot read fixture PID: {error}"),
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("fixture must publish a complete PID")
}
