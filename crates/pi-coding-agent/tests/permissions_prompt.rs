//! Behavioral tests for `coding-agent/src/core/permissions/{orchestrator,mcp-permission-prompt}.ts`.

use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};

use pi_ai::request::BoxFuture;
use pi_coding_agent::core::desktop_notification::{DesktopNotification, DesktopNotificationHandle, DesktopNotifier};
use pi_coding_agent::core::extensions::types::{HandlerError, ToolCallEvent, ToolCallEventResult, ToolName};
use pi_coding_agent::core::permissions::mcp_permission_prompt::{
    PermissionPromptCaller, PermissionPromptDecision, PermissionPromptHandlerOptions, PermissionPromptInput,
    create_permission_prompt_handler, parse_permission_prompt_decision,
};
use pi_coding_agent::core::permissions::orchestrator::{
    ApprovalOrchestratorOptions, ApprovalReviewer, orchestrate_tool_approval,
};
use pi_coding_agent::core::permissions::policy::ApprovalPolicy;
use pi_coding_agent::core::rule_store::{PermissionRuleDestination, PermissionRuleStore};
use serde_json::{Map, Value, json};

fn event(input: Map<String, Value>) -> ToolCallEvent {
    ToolCallEvent {
        tool_name: ToolName::Bash,
        tool_call_id: "toolu_01".to_owned(),
        bypass_permissions: false,
        input: JsValue::from(Value::Object(input)).as_object().unwrap().clone(),
    }
}

fn bash_event() -> ToolCallEvent {
    event(json!({ "command": "git status" }).as_object().unwrap().clone())
}

fn reviewer(result: Option<ToolCallEventResult>) -> ApprovalReviewer {
    Arc::new(move || {
        let result = result.clone();
        Box::pin(async move { Ok(result) }) as BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
    })
}

fn counting_reviewer(result: Option<ToolCallEventResult>, calls: Arc<Mutex<usize>>) -> ApprovalReviewer {
    Arc::new(move || {
        let result = result.clone();
        let calls = Arc::clone(&calls);
        Box::pin(async move {
            *calls.lock().unwrap() += 1;
            Ok(result)
        }) as BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
    })
}

fn blocked(reason: &str) -> Option<ToolCallEventResult> {
    Some(ToolCallEventResult {
        block: Some(true),
        input: None,
        reason: Some(reason.to_owned().into()),
        terminate: None,
    })
}

#[tokio::test]
async fn routes_on_request_approvals_to_the_reviewer() {
    let calls = Arc::new(Mutex::new(0));
    let result = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
        policy: ApprovalPolicy::OnRequest,
        approval_required: true,
        reviewer: Some(counting_reviewer(blocked("blocked"), Arc::clone(&calls))),
        hook_reviewer: None,
        llm_reviewer: None,
    })
    .await
    .expect("approval");
    assert_eq!(result, blocked("blocked"));
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn blocks_never_without_invoking_the_reviewer() {
    let calls = Arc::new(Mutex::new(0));
    let result = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
        policy: ApprovalPolicy::Never,
        approval_required: true,
        reviewer: Some(counting_reviewer(None, Arc::clone(&calls))),
        hook_reviewer: None,
        llm_reviewer: None,
    })
    .await
    .expect("approval");
    assert_eq!(result, blocked("Blocked by approval policy: never"));
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn allows_auto_approve_without_invoking_the_reviewer() {
    let calls = Arc::new(Mutex::new(0));
    let result = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
        policy: ApprovalPolicy::AutoApprove,
        approval_required: true,
        reviewer: Some(counting_reviewer(blocked("blocked"), Arc::clone(&calls))),
        hook_reviewer: None,
        llm_reviewer: None,
    })
    .await
    .expect("approval");
    assert_eq!(result, None);
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn allows_actions_that_do_not_require_approval() {
    let hook_calls = Arc::new(Mutex::new(0));
    let reviewer_calls = Arc::new(Mutex::new(0));
    let llm_calls = Arc::new(Mutex::new(0));
    let result = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
        policy: ApprovalPolicy::OnRequest,
        approval_required: false,
        reviewer: Some(counting_reviewer(
            blocked("reviewer blocked"),
            Arc::clone(&reviewer_calls),
        )),
        hook_reviewer: Some(counting_reviewer(blocked("hook blocked"), Arc::clone(&hook_calls))),
        llm_reviewer: Some(counting_reviewer(blocked("llm blocked"), Arc::clone(&llm_calls))),
    })
    .await
    .expect("approval");
    assert_eq!(result, None);
    assert_eq!(*hook_calls.lock().unwrap(), 0);
    assert_eq!(*reviewer_calls.lock().unwrap(), 0);
    assert_eq!(*llm_calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn skips_the_llm_reviewer_when_a_hook_reviewer_decides() {
    let llm_calls = Arc::new(Mutex::new(0));
    let result = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
        policy: ApprovalPolicy::OnRequest,
        approval_required: true,
        reviewer: None,
        hook_reviewer: Some(reviewer(Some(ToolCallEventResult {
            block: Some(false),
            ..ToolCallEventResult::default()
        }))),
        llm_reviewer: Some(counting_reviewer(blocked("llm blocked"), Arc::clone(&llm_calls))),
    })
    .await
    .expect("approval");
    assert_eq!(
        result,
        Some(ToolCallEventResult {
            block: Some(false),
            ..ToolCallEventResult::default()
        })
    );
    assert_eq!(*llm_calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn falls_through_to_the_llm_reviewer_when_hooks_do_not_decide() {
    let hook_calls = Arc::new(Mutex::new(0));
    let llm_calls = Arc::new(Mutex::new(0));
    let result = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
        policy: ApprovalPolicy::OnRequest,
        approval_required: true,
        reviewer: None,
        hook_reviewer: Some(counting_reviewer(None, Arc::clone(&hook_calls))),
        llm_reviewer: Some(counting_reviewer(blocked("llm blocked"), Arc::clone(&llm_calls))),
    })
    .await
    .expect("approval");
    assert_eq!(result, blocked("llm blocked"));
    assert_eq!(*hook_calls.lock().unwrap(), 1);
    assert_eq!(*llm_calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn uses_the_llm_reviewer_without_a_hook_reviewer() {
    let result = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
        policy: ApprovalPolicy::OnRequest,
        approval_required: true,
        reviewer: None,
        hook_reviewer: None,
        llm_reviewer: Some(reviewer(blocked("llm blocked"))),
    })
    .await
    .expect("approval");
    assert_eq!(result, blocked("llm blocked"));
}

// ---------------------------------------------------------------------------
// Permission prompt decision parsing
// ---------------------------------------------------------------------------

#[test]
fn parses_allow_decisions() {
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!("{\"behavior\":\"allow\"}"))),
        Some(PermissionPromptDecision::Allow {
            updated_input: None,
            updated_permissions: None,
        })
    );
}

#[test]
fn parses_allow_decisions_with_updated_input() {
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!(
            "{\"behavior\":\"allow\",\"updatedInput\":{\"command\":\"git status --short\"}}"
        ))),
        Some(PermissionPromptDecision::Allow {
            updated_input: Some(
                JsValue::from(json!({ "command": "git status --short" }))
                    .as_object()
                    .unwrap()
                    .clone()
            ),
            updated_permissions: None,
        })
    );
}

#[test]
fn parses_allow_decisions_with_updated_permissions() {
    let decision = parse_permission_prompt_decision(&JsValue::from(&json!(
        "{\"behavior\":\"allow\",\"updatedPermissions\":[{\"type\":\"addRules\",\"destination\":\"session\",\"behavior\":\"allow\",\"rules\":[\"git status\"]}]}"
    )))
    .expect("decision");
    let PermissionPromptDecision::Allow {
        updated_permissions: Some(updates),
        ..
    } = decision
    else {
        panic!("expected allow with permissions");
    };
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].destination, PermissionRuleDestination::Session);
    assert_eq!(updates[0].rules, vec!["git status".to_owned()]);
}

#[test]
fn parses_deny_decisions() {
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!(
            "{\"behavior\":\"deny\",\"message\":\"blocked\"}"
        ))),
        Some(PermissionPromptDecision::Deny {
            message: "blocked".to_owned().into()
        })
    );
}

#[test]
fn falls_back_for_malformed_decisions() {
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!("not json"))),
        None
    );
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!("{\"updatedInput\":{}}"))),
        None
    );
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!("{\"behavior\":\"prompt\"}"))),
        None
    );
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!("{\"behavior\":\"allow\",\"updatedInput\":[]}"))),
        None
    );
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!("{\"behavior\":\"deny\",\"message\":\"\"}"))),
        None
    );
}

#[test]
fn parses_decisions_from_text_content_records() {
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!({
            "content": [{ "type": "text", "text": "{\"behavior\":\"deny\",\"message\":\"nope\"}" }]
        }))),
        Some(PermissionPromptDecision::Deny {
            message: "nope".to_owned().into()
        })
    );
    // A record without text content is used directly.
    assert_eq!(
        parse_permission_prompt_decision(&JsValue::from(&json!({ "behavior": "allow" }))),
        Some(PermissionPromptDecision::Allow {
            updated_input: None,
            updated_permissions: None,
        })
    );
}

// ---------------------------------------------------------------------------
// Permission prompt handler
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingNotifier {
    notifications: Mutex<Vec<DesktopNotification>>,
    closed: Mutex<usize>,
}

struct RecordingHandle(Arc<RecordingNotifier>);

impl DesktopNotificationHandle for RecordingHandle {
    fn close(&self) {
        *self.0.closed.lock().unwrap() += 1;
    }
}

impl DesktopNotifier for RecordingNotifier {
    fn notify(&self, notification: &DesktopNotification) -> Option<Box<dyn DesktopNotificationHandle>> {
        self.notifications.lock().unwrap().push(notification.clone());
        Some(Box::new(RecordingHandle(Arc::new(RecordingNotifier::default()))))
    }
}

struct SharedNotifier(Arc<RecordingNotifier>);

impl DesktopNotifier for SharedNotifier {
    fn notify(&self, notification: &DesktopNotification) -> Option<Box<dyn DesktopNotificationHandle>> {
        self.0.notifications.lock().unwrap().push(notification.clone());
        Some(Box::new(SharedHandle(Arc::clone(&self.0))))
    }
}

struct SharedHandle(Arc<RecordingNotifier>);

impl DesktopNotificationHandle for SharedHandle {
    fn close(&self) {
        *self.0.closed.lock().unwrap() += 1;
    }
}

type Calls = Arc<Mutex<Vec<(String, PermissionPromptInput)>>>;

fn caller(response: Result<Value, String>, calls: Calls) -> PermissionPromptCaller {
    Arc::new(move |tool: &str, input: PermissionPromptInput| {
        let response = response.clone();
        let calls = Arc::clone(&calls);
        let tool = tool.to_owned();
        Box::pin(async move {
            calls.lock().unwrap().push((tool, input));
            response
                .map(JsValue::from)
                .map_err(pi_js_compat::js_json::JsString::from)
        })
    })
}

fn handler(
    call_tool: PermissionPromptCaller,
    permission_prompt_tool: Option<&str>,
    desktop_notifier: Option<Arc<dyn DesktopNotifier>>,
    rule_store: Option<Arc<Mutex<PermissionRuleStore>>>,
) -> pi_coding_agent::core::permissions::mcp_permission_prompt::PermissionPromptHandler {
    create_permission_prompt_handler(PermissionPromptHandlerOptions {
        permission_prompt_tool: permission_prompt_tool.map(str::to_owned),
        cwd: "/repo".to_owned().into(),
        call_tool,
        desktop_notifier,
        rule_store,
    })
}

#[tokio::test]
async fn allows_tool_calls_without_further_prompting() {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let handler = handler(
        caller(Ok(json!("{\"behavior\":\"allow\"}")), Arc::clone(&calls)),
        Some("mcp__approval__prompt"),
        None,
        None,
    );
    let mut event = bash_event();
    assert_eq!(handler(&mut event).await, None);
    assert_eq!(
        event.input,
        JsValue::from(json!({ "command": "git status" }))
            .as_object()
            .unwrap()
            .clone()
    );

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "mcp__approval__prompt");
    assert_eq!(
        calls[0].1,
        PermissionPromptInput {
            cwd: "/repo".to_owned().into(),
            input: JsValue::from(json!({ "command": "git status" }))
                .as_object()
                .unwrap()
                .clone(),
            tool_name: "bash".to_owned(),
            tool_use_id: "toolu_01".to_owned(),
        }
    );
}

#[tokio::test]
async fn sends_a_desktop_notification_before_the_prompt() {
    let notifier = Arc::new(RecordingNotifier::default());
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let handler = handler(
        caller(Ok(json!("{\"behavior\":\"allow\"}")), Arc::clone(&calls)),
        Some("mcp__approval__prompt"),
        Some(Arc::new(SharedNotifier(Arc::clone(&notifier)))),
        None,
    );
    let mut event = bash_event();
    assert_eq!(handler(&mut event).await, None);

    let notifications = notifier.notifications.lock().unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].body, "Permission approval needed for bash in /repo.");
    assert_eq!(notifications[0].title, "Pi permission approval needed");
    assert_eq!(notifications[0].expire_time_ms, Some(999_999));
    assert!(!notifications[0].body.contains("secret-token"));
    assert_eq!(*notifier.closed.lock().unwrap(), 1);
}

#[tokio::test]
async fn allows_tool_calls_with_updated_input_in_place() {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let handler = handler(
        caller(
            Ok(json!(
                "{\"behavior\":\"allow\",\"updatedInput\":{\"command\":\"git status --short\"}}"
            )),
            Arc::clone(&calls),
        ),
        Some("mcp__approval__prompt"),
        None,
        None,
    );
    let mut event = bash_event();
    assert_eq!(handler(&mut event).await, None);
    assert_eq!(
        event.input,
        JsValue::from(json!({ "command": "git status --short" }))
            .as_object()
            .unwrap()
            .clone()
    );
}

#[tokio::test]
async fn blocks_denied_tool_calls() {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let handler = handler(
        caller(
            Ok(json!("{\"behavior\":\"deny\",\"message\":\"blocked\"}")),
            Arc::clone(&calls),
        ),
        Some("mcp__approval__prompt"),
        None,
        None,
    );
    let mut event = bash_event();
    assert_eq!(handler(&mut event).await, blocked("blocked"));
}

#[tokio::test]
async fn falls_back_when_the_prompt_returns_malformed_output() {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let handler = handler(
        caller(Ok(json!("{\"behavior\":\"prompt\"}")), Arc::clone(&calls)),
        Some("mcp__approval__prompt"),
        None,
        None,
    );
    let mut event = bash_event();
    assert_eq!(handler(&mut event).await, None);
}

#[tokio::test]
async fn falls_back_without_a_configured_prompt_tool_or_with_an_invalid_name() {
    for configured in [None, Some("approval_prompt")] {
        let calls: Calls = Arc::new(Mutex::new(Vec::new()));
        let handler = handler(
            caller(Ok(json!("{\"behavior\":\"allow\"}")), Arc::clone(&calls)),
            configured,
            None,
            None,
        );
        let mut event = bash_event();
        assert_eq!(handler(&mut event).await, None);
        assert!(calls.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn falls_back_when_the_prompt_call_fails() {
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let handler = handler(
        caller(Err("mcp offline".to_owned()), Arc::clone(&calls)),
        Some("mcp__approval__prompt"),
        None,
        None,
    );
    let mut event = bash_event();
    assert_eq!(handler(&mut event).await, None);
}

#[tokio::test]
async fn session_allow_rules_skip_future_prompt_calls() {
    let rule_store = Arc::new(Mutex::new(PermissionRuleStore::new()));
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let handler = handler(
        caller(
            Ok(json!(
                "{\"behavior\":\"allow\",\"updatedPermissions\":[{\"type\":\"addRules\",\"destination\":\"session\",\"behavior\":\"allow\",\"rules\":[\"git status\"]}]}"
            )),
            Arc::clone(&calls),
        ),
        Some("mcp__approval__prompt"),
        None,
        Some(Arc::clone(&rule_store)),
    );

    let mut first = bash_event();
    assert_eq!(handler(&mut first).await, None);
    let mut second = bash_event();
    assert_eq!(handler(&mut second).await, None);
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn non_session_allow_rules_do_not_skip_follow_up_prompts() {
    let rule_store = Arc::new(Mutex::new(PermissionRuleStore::with_writer(Box::new(
        |_options: &pi_coding_agent::core::rule_store::WritePermissionRulesOptions| {},
    ))));
    let calls: Calls = Arc::new(Mutex::new(Vec::new()));
    let handler = handler(
        caller(
            Ok(json!(
                "{\"behavior\":\"allow\",\"updatedPermissions\":[{\"type\":\"addRules\",\"destination\":\"userSettings\",\"behavior\":\"allow\",\"rules\":[\"git status\"]}]}"
            )),
            Arc::clone(&calls),
        ),
        Some("mcp__approval__prompt"),
        None,
        Some(Arc::clone(&rule_store)),
    );

    let mut first = bash_event();
    assert_eq!(handler(&mut first).await, None);
    let mut second = bash_event();
    assert_eq!(handler(&mut second).await, None);
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn reviewer_failures_propagate_instead_of_allowing_the_tool_call() {
    let failing: ApprovalReviewer = Arc::new(|| {
        Box::pin(async { Err(HandlerError::new("reviewer unavailable")) })
            as BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
    });
    let error = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
        policy: ApprovalPolicy::OnRequest,
        approval_required: true,
        reviewer: Some(failing),
        hook_reviewer: None,
        llm_reviewer: None,
    })
    .await
    .expect_err("reviewer failure must propagate");
    assert_eq!(error.message, "reviewer unavailable");
}
