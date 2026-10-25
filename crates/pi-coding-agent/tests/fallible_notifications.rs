use crate::shared_oracle as oracle;

use pi_coding_agent::core::desktop_notification::{DesktopNotification, DesktopNotificationHandle, DesktopNotifier};
use pi_coding_agent::core::extensions::types::{ExtensionUiContext, NotifyType};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::node_error::NativeNodeError;

struct RejectingNotification(CallbackError);
impl DesktopNotifier for RejectingNotification {
    fn notify(&self, _: &DesktopNotification) -> Result<Option<Box<dyn DesktopNotificationHandle>>, CallbackError> {
        Err(self.0.clone())
    }
}
impl DesktopNotificationHandle for RejectingNotification {
    fn close(&self) -> Result<(), CallbackError> {
        Err(self.0.clone())
    }
}

impl ExtensionUiContext for RejectingNotification {
    fn notify(&self, _: &JsString, _: Option<NotifyType>) -> Result<(), CallbackError> {
        Err(self.0.clone())
    }
}

struct ClosingNotification(CallbackError);
impl DesktopNotifier for ClosingNotification {
    fn notify(&self, _: &DesktopNotification) -> Result<Option<Box<dyn DesktopNotificationHandle>>, CallbackError> {
        Ok(Some(Box::new(RejectingNotification(self.0.clone()))))
    }
}

#[test]
fn question_notification_failures_do_not_cancel_panel_settlement() {
    use pi_coding_agent::core::tools::ask_questions::{close_ask_questions_notification, notify_ask_questions_waiting};
    let error = CallbackError::from_rejection(AbortReason::Value(JsValue::String(JsString::from_utf16(vec![0xd800]))));
    assert!(notify_ask_questions_waiting(&RejectingNotification(error.clone()), 1).is_none());
    let handle = notify_ask_questions_waiting(&ClosingNotification(error), 2);
    assert!(handle.is_some());
    close_ask_questions_notification(handle);
}

#[tokio::test]
async fn permission_notifier_send_and_close_failures_preserve_the_prompt_decision() {
    use pi_coding_agent::core::extensions::types::{ToolCallEvent, ToolName};
    use pi_coding_agent::core::permissions::mcp_permission_prompt::{
        PermissionPromptHandlerOptions, create_permission_prompt_handler,
    };
    use pi_js_compat::js_json as json;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let reason = JsString::from_utf16(vec![0xd800]);
    let error = CallbackError::from_rejection(AbortReason::Value(JsValue::String(reason.clone())));
    let mut oracle = oracle::TsOracle::spawn(
        "permission-notification-errors-oracle.ts",
        "packages/coding-agent/src/core/permissions/mcp-permission-prompt.ts",
    );
    let notifiers: Vec<Arc<dyn DesktopNotifier>> = vec![
        Arc::new(RejectingNotification(error.clone())),
        Arc::new(ClosingNotification(error)),
    ];
    for (phase, notifier) in ["send", "close"].into_iter().zip(notifiers) {
        let expected = oracle.request(&json!({"id":1,"phase":phase,"reason":reason}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let expected = &expected["output"];
        assert_eq!(
            expected["logs"],
            json!([[
                format!("Failed to {phase} permission prompt desktop notification:"),
                reason
            ]])
        );
        assert_eq!(expected["sends"], json!(1));
        assert_eq!(expected["closes"], json!(usize::from(phase == "close")));
        let calls = Arc::new(AtomicUsize::new(0));
        let output = Arc::clone(&calls);
        let handler = create_permission_prompt_handler(PermissionPromptHandlerOptions {
            permission_prompt_tool: Some("mcp__approval__prompt".into()),
            cwd: "/repo".into(),
            desktop_notifier: Some(notifier),
            rule_store: None,
            call_tool: Arc::new(move |_, _| {
                output.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(json!({"behavior":"deny","message":"denied"})) })
            }),
        });
        let mut event = ToolCallEvent {
            tool_name: ToolName::Bash,
            tool_call_id: "fixture".into(),
            bypass_permissions: false,
            input: Default::default(),
        };
        let decision = handler(&mut event).await.expect("prompt decision");
        assert_eq!(decision.block, Some(true));
        assert_eq!(decision.reason, Some("denied".into()));
        assert_eq!(json!(calls.load(Ordering::SeqCst)), expected["calls"]);
        assert_eq!(
            pi_js_compat::js_json::to_value(&decision).unwrap(),
            expected["decision"]
        );
    }
}

#[test]
fn native_notification_boundaries_retain_typed_errors_and_arbitrary_throws() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let reasons = [
        AbortReason::Error(
            NativeNodeError::new("TypeError", raw.clone()).with_property("code", JsValue::String("E_FIXTURE".into())),
        ),
        AbortReason::Value(JsValue::String(raw)),
        AbortReason::Value(JsValue::Null),
    ];
    for reason in reasons {
        let expected = CallbackError::from_rejection(reason);
        let host = RejectingNotification(expected.clone());
        let desktop: &dyn DesktopNotifier = &host;
        let handle: &dyn DesktopNotificationHandle = &host;
        let ui: &dyn ExtensionUiContext = &host;
        let notification = DesktopNotification {
            title: "fixture".into(),
            body: "fixture".into(),
            expire_time_ms: None,
            urgency: None,
        };
        let errors = [
            desktop.notify(&notification).err().expect("desktop rejection"),
            handle.close().expect_err("close rejection"),
            ui.notify(&"fixture".into(), None).expect_err("UI rejection"),
        ];
        for error in errors {
            assert_eq!(error.thrown_value(), expected.thrown_value());
        }
    }
}
