//! Behavioral tests for the bundled `loop`, `codex-fast`, and `effort` extensions.

use pi_js_compat::js_json::JsString;
#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/extensions.rs"]
mod support;

use std::sync::Arc;

use bundled::{Harness, HarnessOptions, test_model};
use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::extensions::types::{
    CustomMessageContent, ExtensionEvent, MessageDelivery, NotifyType, SessionShutdownEvent, SessionStartEvent,
    SessionStartReason, ThinkingLevelSelectEvent,
};
use pi_coding_agent::core::extensions::{Extension, ExtensionRuntime, create_extension_runtime};
use pi_coding_agent::core::system_prompt::BuildSystemPromptOptions;
use pi_js_compat::js_json as json;

async fn load_loop(runtime: &ExtensionRuntime, cwd: &str) -> Extension {
    support::extension(runtime, cwd, "<first-party:loop>", |pi| {
        pi_coding_agent::bundled_extensions::r#loop::loop_extension(pi).expect("loop extension")
    })
    .await
}

async fn load_fast(
    runtime: &ExtensionRuntime,
    cwd: &str,
    authority: &pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority,
) -> Extension {
    let options = pi_coding_agent::bundled_extensions::codex_fast::CodexFastOptions {
        authority: authority.clone(),
    };
    support::extension(runtime, cwd, "<first-party:codex-fast>", move |pi| {
        pi_coding_agent::bundled_extensions::codex_fast::codex_fast_extension(pi, &options)
            .expect("codex-fast extension")
    })
    .await
}

async fn load_effort(runtime: &ExtensionRuntime, cwd: &str) -> Extension {
    support::extension(runtime, cwd, "<first-party:effort>", |pi| {
        pi_coding_agent::bundled_extensions::effort::effort_extension(pi).expect("effort extension")
    })
    .await
}

async fn build_harness(extension: Extension, runtime: ExtensionRuntime, options: HarnessOptions) -> Harness {
    Harness::new(vec![extension], runtime, options).await
}

fn tool(extension: &Extension, name: &str) -> pi_coding_agent::core::extensions::types::ToolDefinition {
    extension.read().tools.get(name).expect("tool").definition.clone()
}

fn text(result: &pi_agent_core::AgentToolResult) -> JsString {
    let TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("text result");
    };
    text.text.clone()
}

#[tokio::test(start_paused = true)]
async fn loop_registers_command_and_tool() {
    let runtime = create_extension_runtime();
    let extension = load_loop(&runtime, "/repo").await;
    let loop_tool = tool(&extension, "loop");
    assert_eq!(loop_tool.name, "loop");
    assert_eq!(loop_tool.label, "Loop");
    assert_eq!(loop_tool.approval_required, Some(true));
    assert_eq!(
        loop_tool.description,
        "Start, stop, or inspect a recurring prompt injected into this Pi session."
    );
    assert_eq!(
        loop_tool.prompt_guidelines,
        Some(vec![
            JsString::from(
                "Use loop only when the user wants recurring follow-up prompts injected into the current session."
            ),
            JsString::from("Prefer action=status before changing an existing loop when unsure."),
            JsString::from("Use action=stop when the recurring prompts are no longer needed."),
        ])
    );
    assert_eq!(
        pi_js_compat::js_json::JsValue::from(loop_tool.parameters),
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "anyOf": [
                        {"type": "string", "const": "start"},
                        {"type": "string", "const": "stop"},
                        {"type": "string", "const": "status"}
                    ]
                },
                "intervalSeconds": {
                    "type": "number",
                    "description": "Interval in seconds. Required for action=start."
                },
                "prompt": {
                    "type": "string",
                    "description": "Prompt to inject on every interval. Required for action=start."
                }
            }
        })
    );
    assert_eq!(
        extension
            .read()
            .commands
            .get(&pi_js_compat::js_json::JsString::from("loop"))
            .expect("loop command")
            .description
            .clone(),
        Some(JsString::from(
            "Inject a recurring prompt (/loop <interval> <prompt> | /loop status | /loop stop)."
        ))
    );
}

#[tokio::test(start_paused = true)]
async fn loop_command_injects_prompt_at_interval() {
    let runtime = create_extension_runtime();
    let extension = load_loop(&runtime, "/repo").await;
    let harness = build_harness(extension.clone(), runtime.clone(), HarnessOptions::new("/repo")).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("loop"))
        .expect("loop command")
        .handler
        .clone();

    command("10s check progress".to_owned().into(), harness.command_context())
        .await
        .expect("loop command");
    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("Loop started every 10s"), Some(NotifyType::Info)))
    );
    assert_eq!(harness.ui.editor_texts.lock().unwrap().as_slice(), [""]);
    assert!(harness.actions.messages.lock().unwrap().is_empty());
    tokio::task::yield_now().await;

    tokio::time::advance(std::time::Duration::from_millis(9_999)).await;
    tokio::task::yield_now().await;
    assert!(harness.actions.messages.lock().unwrap().is_empty());

    tokio::time::advance(std::time::Duration::from_millis(1)).await;
    tokio::task::yield_now().await;
    let messages = harness.actions.messages.lock().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, CustomMessageContent::Text("check progress".into()));
    assert_eq!(messages[0].custom_type, "loop");
    assert!(messages[0].display);
    let session_id =
        pi_coding_agent::core::session_manager::lock_session_manager(&harness.context().session_manager().unwrap())
            .get_session_id()
            .clone();
    assert_eq!(
        messages[0].details,
        Some(json!({"generation": 1, "sessionId": session_id}))
    );
    let options = harness.actions.message_options.lock().unwrap();
    assert_eq!(
        options[0].as_ref().and_then(|options| options.deliver_as),
        Some(MessageDelivery::FollowUp)
    );
    assert_eq!(options[0].as_ref().and_then(|options| options.trigger_turn), Some(true));
}

#[tokio::test(start_paused = true)]
async fn loop_command_stops_active_loop() {
    let runtime = create_extension_runtime();
    let extension = load_loop(&runtime, "/repo").await;
    let harness = build_harness(extension.clone(), runtime.clone(), HarnessOptions::new("/repo")).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("loop"))
        .expect("loop command")
        .handler
        .clone();

    command("5s check progress".to_owned().into(), harness.command_context())
        .await
        .expect("start");
    command("stop".to_owned().into(), harness.command_context())
        .await
        .expect("stop");
    tokio::time::advance(std::time::Duration::from_millis(5_000)).await;
    tokio::task::yield_now().await;

    assert!(harness.actions.messages.lock().unwrap().is_empty());
    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("Loop stopped"), Some(NotifyType::Info)))
    );
}

#[tokio::test(start_paused = true)]
async fn loop_tool_starts_and_stops_loop() {
    let runtime = create_extension_runtime();
    let extension = load_loop(&runtime, "/repo").await;
    let harness = build_harness(extension.clone(), runtime.clone(), HarnessOptions::new("/repo")).await;
    let loop_tool = tool(&extension, "loop");
    let ctx = harness.context();

    let start = (loop_tool.execute)(
        "loop-call".to_owned(),
        json!({"action": "start", "intervalSeconds": 3, "prompt": "continue"}),
        None,
        None,
        Some(ctx.clone()),
    )
    .await
    .expect("start");
    assert!(text(&start).contains("Loop started"));
    assert_eq!(
        start.details,
        Some(json!({"action": "start", "active": true, "intervalMs": 3000, "prompt": "continue"}))
    );

    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(3_000)).await;
    tokio::task::yield_now().await;
    assert_eq!(harness.actions.messages.lock().unwrap().len(), 1);

    let stop = (loop_tool.execute)("loop-call".to_owned(), json!({"action": "stop"}), None, None, Some(ctx))
        .await
        .expect("stop");
    assert_eq!(stop.details, Some(json!({"action": "stop", "active": false})));
    tokio::time::advance(std::time::Duration::from_millis(3_000)).await;
    tokio::task::yield_now().await;
    assert_eq!(harness.actions.messages.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn loop_shutdown_clears_timer() {
    let runtime = create_extension_runtime();
    let extension = load_loop(&runtime, "/repo").await;
    let harness = build_harness(extension.clone(), runtime.clone(), HarnessOptions::new("/repo")).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("loop"))
        .expect("loop command")
        .handler
        .clone();

    command("2s keep going".to_owned().into(), harness.command_context())
        .await
        .expect("start");
    let mut event = ExtensionEvent::SessionShutdown(SessionShutdownEvent {
        reason: pi_coding_agent::core::extensions::types::SessionShutdownReason::Quit,
        target_session_file: None,
    });
    harness.runner.emit(&mut event).await;
    tokio::time::advance(std::time::Duration::from_millis(2_000)).await;
    tokio::task::yield_now().await;

    assert!(harness.actions.messages.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn loop_command_reports_usage_and_missing_prompt() {
    let runtime = create_extension_runtime();
    let extension = load_loop(&runtime, "/repo").await;
    let harness = build_harness(extension.clone(), runtime.clone(), HarnessOptions::new("/repo")).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("loop"))
        .expect("loop command")
        .handler
        .clone();

    command("bogus".to_owned().into(), harness.command_context())
        .await
        .expect("usage");
    assert_eq!(
        harness.ui.last_notification(),
        Some((
            JsString::from("Usage: /loop <interval> <prompt> | /loop status | /loop stop"),
            Some(NotifyType::Error)
        ))
    );

    command("5s".to_owned().into(), harness.command_context())
        .await
        .expect("missing prompt");
    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("Loop prompt is required."), Some(NotifyType::Error)))
    );
}

#[tokio::test(start_paused = true)]
async fn fast_toggles_priority_and_updates_status() {
    let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
    let runtime = create_extension_runtime();
    let extension = load_fast(&runtime, "/repo", &authority).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("fast"))
        .expect("fast command")
        .handler
        .clone();

    command(String::new().into(), harness.command_context())
        .await
        .expect("toggle on");
    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("Fast mode: on"), Some(NotifyType::Info)))
    );
    assert_eq!(harness.ui.status("codex-fast"), Some(JsString::from("fast")));

    command(String::new().into(), harness.command_context())
        .await
        .expect("toggle off");
    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("Fast mode: off"), Some(NotifyType::Info)))
    );
    assert_eq!(harness.ui.status("codex-fast"), None);
    assert_eq!(harness.ui.editor_texts.lock().unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn fast_persists_explicit_on_and_off() {
    let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
    let runtime = create_extension_runtime();
    let extension = load_fast(&runtime, "/repo", &authority).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(false));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("fast"))
        .expect("fast command")
        .handler
        .clone();

    command("on".to_owned().into(), harness.command_context())
        .await
        .expect("on");
    command("off".to_owned().into(), harness.command_context())
        .await
        .expect("off");

    assert_eq!(
        harness.actions.entries.lock().unwrap().as_slice(),
        [
            (
                JsString::from("codex-fast-mode"),
                Some(json!({"serviceTier": "priority"}))
            ),
            (JsString::from("codex-fast-mode"), Some(json!({"serviceTier": null}))),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn fast_ultra_adds_service_tier_until_disabled() {
    let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
    let runtime = create_extension_runtime();
    let extension = load_fast(&runtime, "/repo", &authority).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("fast"))
        .expect("fast command")
        .handler
        .clone();

    command("ultra".to_owned().into(), harness.command_context())
        .await
        .expect("ultra");
    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("Fast mode: ultra"), Some(NotifyType::Info)))
    );
    assert_eq!(harness.ui.status("codex-fast"), Some(JsString::from("fast ultra")));

    assert_eq!(
        harness
            .runner
            .emit_before_provider_request(json!({"model": "test-model"}))
            .await,
        json!({"model": "test-model", "service_tier": "ultrafast"})
    );

    command("off".to_owned().into(), harness.command_context())
        .await
        .expect("off");
    assert_eq!(
        harness
            .runner
            .emit_before_provider_request(json!({"model": "test-model"}))
            .await,
        json!({"model": "test-model"})
    );
}

#[tokio::test(start_paused = true)]
async fn fast_rejects_unsupported_providers() {
    let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
    let runtime = create_extension_runtime();
    let extension = load_fast(&runtime, "/repo", &authority).await;
    let mut options = HarnessOptions::new("/repo");
    let mut model = test_model(false);
    model.provider = "anthropic".to_owned();
    model.api = "anthropic-messages".to_owned();
    options.model = Some(model);
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("fast"))
        .expect("fast command")
        .handler
        .clone();

    command("on".to_owned().into(), harness.command_context())
        .await
        .expect("on");

    assert_eq!(
        harness.ui.last_notification(),
        Some((
            JsString::from("Fast mode requires openai-codex or openai-codex-gc"),
            Some(NotifyType::Warning)
        ))
    );
    assert_eq!(harness.ui.status("codex-fast"), None);
}

#[tokio::test(start_paused = true)]
async fn fast_skips_non_object_payload_without_losing_mode() {
    let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
    let runtime = create_extension_runtime();
    let extension = load_fast(&runtime, "/repo", &authority).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("fast"))
        .expect("fast command")
        .handler
        .clone();
    command("on".to_owned().into(), harness.command_context())
        .await
        .expect("on");

    assert_eq!(
        harness.runner.emit_before_provider_request(json!("unexpected")).await,
        json!("unexpected")
    );
    assert_eq!(
        harness.ui.last_notification(),
        Some((
            JsString::from("Fast mode skipped: provider payload is not an object"),
            Some(NotifyType::Warning)
        ))
    );
    assert_eq!(
        authority.service_tier(),
        Some(pi_coding_agent::bundled_extensions::codex_fast::FastServiceTier::Priority)
    );
}

#[tokio::test(start_paused = true)]
async fn fast_shares_authority_and_blocks_child_commands() {
    let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
    let main_runtime = create_extension_runtime();
    let main = load_fast(&main_runtime, "/repo", &authority).await;
    let child_runtime = create_extension_runtime();
    let child = load_fast(&child_runtime, "/repo", &authority).await;
    let mut main_options = HarnessOptions::new("/repo");
    main_options.model = Some(test_model(true));
    let main = build_harness(main, main_runtime, main_options).await;
    let mut child_options = HarnessOptions::new("/repo");
    child_options.model = Some(test_model(true));
    child_options.multi_agent_agent_id = Some("child-agent".to_owned().into());
    let child = build_harness(child, child_runtime, child_options).await;

    let main_command = main.runner.get_command("fast").expect("fast command").handler.clone();
    let child_command = child.runner.get_command("fast").expect("fast command").handler.clone();

    main_command("on".to_owned().into(), main.command_context())
        .await
        .expect("main on");
    assert_eq!(
        child
            .runner
            .emit_before_provider_request(json!({"model": "test-model"}))
            .await,
        json!({"model": "test-model", "service_tier": "priority"})
    );

    child_command("off".to_owned().into(), child.command_context())
        .await
        .expect("child off");
    assert_eq!(
        child.ui.last_notification(),
        Some((
            JsString::from("Fast mode is controlled by the main thread"),
            Some(NotifyType::Warning)
        ))
    );
    assert_eq!(
        authority.service_tier(),
        Some(pi_coding_agent::bundled_extensions::codex_fast::FastServiceTier::Priority)
    );
}

#[tokio::test(start_paused = true)]
async fn fast_restores_persisted_and_configured_modes() {
    let cwd = "/repo";
    let mut session =
        pi_coding_agent::core::session_manager::SessionManager::in_memory(Some(&JsString::from(cwd)), None)
            .expect("session");
    session
        .append_custom_entry(
            &JsString::from("codex-fast-mode"),
            Some(json!({"serviceTier": "ultrafast"})),
        )
        .expect("custom entry");
    let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
    let runtime = create_extension_runtime();
    let extension = load_fast(&runtime, cwd, &authority).await;
    let mut options = HarnessOptions::new(cwd);
    options.model = Some(test_model(true));
    options.session_manager = Arc::new(std::sync::Mutex::new(session));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;

    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
        reason: SessionStartReason::Resume,
        previous_session_file: None,
    });
    harness.runner.emit(&mut event).await;
    assert_eq!(
        authority.service_tier(),
        Some(pi_coding_agent::bundled_extensions::codex_fast::FastServiceTier::Ultrafast)
    );

    let authority = pi_coding_agent::bundled_extensions::codex_fast::FastModeAuthority::new();
    let runtime = create_extension_runtime();
    let extension = load_fast(&runtime, cwd, &authority).await;
    let mut options = HarnessOptions::new(cwd);
    options.model = Some(test_model(true));
    options.settings =
        Some(pi_js_compat::js_json::from_value(&json!({"defaultCodexFastMode": "priority"})).expect("settings"));
    let harness = build_harness(extension, runtime.clone(), options).await;
    harness.runner.emit(&mut event).await;
    assert_eq!(
        authority.service_tier(),
        Some(pi_coding_agent::bundled_extensions::codex_fast::FastServiceTier::Priority)
    );
}

#[tokio::test(start_paused = true)]
async fn effort_registers_command_and_opens_selector() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    assert_eq!(
        extension
            .read()
            .commands
            .get(&pi_js_compat::js_json::JsString::from("effort"))
            .expect("effort command")
            .description
            .clone(),
        Some(JsString::from("Set model effort level (depends on selected model)"))
    );
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    harness.ui.select_result(Some("high"));
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("effort"))
        .expect("effort command")
        .handler
        .clone();

    command(String::new().into(), harness.command_context())
        .await
        .expect("effort");

    let calls = harness.ui.select_calls.lock().unwrap();
    assert_eq!(calls[0].0, "Select effort");
    assert!(calls[0].1.contains(&JsString::from("off".to_owned())));
    assert!(calls[0].1.contains(&JsString::from("high".to_owned())));
    assert_eq!(*harness.thinking.lock().unwrap(), ThinkingLevel::High);
    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("Effort: high"), Some(NotifyType::Info)))
    );
    assert_eq!(harness.ui.editor_texts.lock().unwrap().as_slice(), [""]);
}

#[tokio::test(start_paused = true)]
async fn effort_only_offers_supported_levels() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(false));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    harness.ui.select_result(Some("off"));
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("effort"))
        .expect("effort command")
        .handler
        .clone();

    command(String::new().into(), harness.command_context())
        .await
        .expect("effort");

    assert_eq!(harness.ui.select_calls.lock().unwrap()[0].1, vec!["off".to_owned()]);
    assert_eq!(*harness.thinking.lock().unwrap(), ThinkingLevel::Off);
}

#[tokio::test(start_paused = true)]
async fn effort_cancelled_selector_changes_nothing() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    harness.ui.select_result(None);
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("effort"))
        .expect("effort command")
        .handler
        .clone();

    command(String::new().into(), harness.command_context())
        .await
        .expect("effort");

    assert_eq!(*harness.thinking.lock().unwrap(), ThinkingLevel::Off);
    assert!(harness.ui.notifications().is_empty());
    assert_eq!(harness.ui.editor_texts.lock().unwrap().as_slice(), [""]);
}

#[tokio::test(start_paused = true)]
async fn effort_rejects_unsupported_levels() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(false));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("effort"))
        .expect("effort command")
        .handler
        .clone();

    command("high".to_owned().into(), harness.command_context())
        .await
        .expect("effort");

    assert_eq!(*harness.thinking.lock().unwrap(), ThinkingLevel::Off);
    assert_eq!(
        harness.ui.last_notification(),
        Some((
            JsString::from("Invalid effort \"high\". Available: off"),
            Some(NotifyType::Warning)
        ))
    );
}

#[tokio::test(start_paused = true)]
async fn effort_reports_missing_model() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let harness = build_harness(extension.clone(), runtime.clone(), HarnessOptions::new("/repo")).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("effort"))
        .expect("effort command")
        .handler
        .clone();

    command("high".to_owned().into(), harness.command_context())
        .await
        .expect("effort");

    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("No model selected"), Some(NotifyType::Warning)))
    );
}

#[tokio::test(start_paused = true)]
async fn effort_injects_proactive_policy_and_restores_explicit_mode() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;

    let result = harness
        .runner
        .emit_before_agent_start(
            &JsString::from("hi"),
            None,
            &JsString::from("base"),
            BuildSystemPromptOptions::default(),
        )
        .await
        .expect("before_agent_start result");
    assert!(
        result
            .system_prompt
            .expect("system prompt")
            .contains("Proactive multi-agent delegation is active.")
    );

    let multi_agent = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("multi-agent"))
        .expect("multi-agent command")
        .handler
        .clone();
    multi_agent("explicit".to_owned().into(), harness.command_context())
        .await
        .expect("explicit");
    assert_eq!(
        harness.actions.entries.lock().unwrap().last().cloned(),
        Some((JsString::from("multi-agent-mode"), Some(json!({"mode": "explicit"}))))
    );

    let result = harness
        .runner
        .emit_before_agent_start(
            &JsString::from("hi"),
            None,
            &JsString::from("base\n\n<multi_agent_mode>Proactive multi-agent delegation is active.</multi_agent_mode>"),
            BuildSystemPromptOptions::default(),
        )
        .await
        .expect("before_agent_start result");
    let prompt = result.system_prompt.expect("system prompt");
    assert!(prompt.contains("Do not spawn sub-agents unless"));
    assert!(!prompt.contains("Proactive multi-agent delegation is active."));
}

#[tokio::test(start_paused = true)]
async fn effort_restores_mode_from_branch() {
    let cwd = "/repo";
    let mut session =
        pi_coding_agent::core::session_manager::SessionManager::in_memory(Some(&JsString::from(cwd)), None)
            .expect("session");
    session
        .append_custom_entry(&JsString::from("multi-agent-mode"), Some(json!({"mode": "explicit"})))
        .expect("entry");
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, cwd).await;
    let mut options = HarnessOptions::new(cwd);
    options.model = Some(test_model(true));
    options.session_manager = Arc::new(std::sync::Mutex::new(session));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;

    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
        reason: SessionStartReason::Resume,
        previous_session_file: None,
    });
    harness.runner.emit(&mut event).await;

    let result = harness
        .runner
        .emit_before_agent_start(
            &JsString::from("hi"),
            None,
            &JsString::from("base"),
            BuildSystemPromptOptions::default(),
        )
        .await
        .expect("before_agent_start result");
    assert!(
        result
            .system_prompt
            .expect("system prompt")
            .contains("Do not spawn sub-agents unless")
    );
}

#[tokio::test(start_paused = true)]
async fn effort_ultra_activates_proactive_and_maps_thinking() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    options.thinking_level = ThinkingLevel::High;
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let command = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("effort"))
        .expect("effort command")
        .handler
        .clone();

    command("ultra".to_owned().into(), harness.command_context())
        .await
        .expect("ultra");

    assert_eq!(*harness.thinking.lock().unwrap(), ThinkingLevel::Ultra);
    assert_eq!(
        harness.actions.entries.lock().unwrap().last().cloned(),
        Some((JsString::from("multi-agent-mode"), Some(json!({"mode": "proactive"}))))
    );
    assert_eq!(
        harness.ui.last_notification(),
        Some((
            JsString::from("Effort: ultra (max + proactive)"),
            Some(NotifyType::Info)
        ))
    );
}

#[tokio::test(start_paused = true)]
async fn effort_explicit_from_ultra_keeps_max_reasoning() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    options.thinking_level = ThinkingLevel::Ultra;
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let multi_agent = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("multi-agent"))
        .expect("multi-agent command")
        .handler
        .clone();

    multi_agent("explicit".to_owned().into(), harness.command_context())
        .await
        .expect("explicit");

    assert_eq!(*harness.thinking.lock().unwrap(), ThinkingLevel::Max);
}

#[tokio::test(start_paused = true)]
async fn effort_thinking_level_ultra_reactivates_proactive() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let multi_agent = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("multi-agent"))
        .expect("multi-agent command")
        .handler
        .clone();
    multi_agent("explicit".to_owned().into(), harness.command_context())
        .await
        .expect("explicit");
    harness.actions.entries.lock().unwrap().clear();

    let mut event = ExtensionEvent::ThinkingLevelSelect(ThinkingLevelSelectEvent {
        level: ThinkingLevel::Ultra,
        previous_level: ThinkingLevel::Max,
    });
    harness.runner.emit(&mut event).await;

    assert_eq!(
        harness.actions.entries.lock().unwrap().last().cloned(),
        Some((JsString::from("multi-agent-mode"), Some(json!({"mode": "proactive"}))))
    );
}

#[tokio::test(start_paused = true)]
async fn effort_child_runtime_cannot_change_mode_or_receive_policy() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    options.multi_agent_agent_id = Some("child-agent".to_owned().into());
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let multi_agent = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("multi-agent"))
        .expect("multi-agent command")
        .handler
        .clone();

    multi_agent("explicit".to_owned().into(), harness.command_context())
        .await
        .expect("explicit");

    assert!(harness.actions.entries.lock().unwrap().is_empty());
    assert_eq!(
        harness.ui.last_notification(),
        Some((
            JsString::from("Multi-agent mode is controlled by the main thread"),
            Some(NotifyType::Warning)
        ))
    );

    assert!(
        harness
            .runner
            .emit_before_agent_start(
                &JsString::from("hi"),
                None,
                &JsString::from("base"),
                BuildSystemPromptOptions::default()
            )
            .await
            .is_none()
    );
}

#[tokio::test(start_paused = true)]
async fn effort_status_dims_only_the_prefix() {
    let runtime = create_extension_runtime();
    let extension = load_effort(&runtime, "/repo").await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(true));
    let harness = build_harness(extension.clone(), runtime.clone(), options).await;
    let theme = pi_coding_agent::modes::interactive::theme::current_theme();

    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
        reason: SessionStartReason::New,
        previous_session_file: None,
    });
    harness.runner.emit(&mut event).await;

    assert_eq!(
        harness.ui.status("multi-agent-mode"),
        Some(JsString::join(
            [
                theme.fg(
                    pi_coding_agent::modes::interactive::theme::ThemeColor::Dim,
                    "multi-agent: "
                ),
                JsString::from("proactive")
            ],
            ""
        ))
    );
}
