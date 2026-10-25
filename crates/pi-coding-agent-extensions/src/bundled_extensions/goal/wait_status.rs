//! Port of `packages/coding-agent/extensions/goal/src/wait-status.ts`.

use crate::core::extensions::types::ExtensionContext;

use super::goal_scheduling::GoalWaitCallbacks;
use super::rendering::AppendSupervisorStatus;

/// `appendGoalSchedulingError`.
pub fn append_goal_scheduling_error(
    append_status: &AppendSupervisorStatus,
    error: &pi_js_compat::js_json::JsString,
    ctx: ExtensionContext,
) {
    let _ = append_status(
        ctx,
        &(pi_js_compat::js_json::JsString::from("Goal wait failed: ") + error),
        None,
        None,
    );
}

/// `createWaitStatusCallbacks`.
pub fn create_wait_status_callbacks(
    append_status: AppendSupervisorStatus,
    ctx: ExtensionContext,
    message: pi_js_compat::js_json::JsString,
) -> GoalWaitCallbacks {
    GoalWaitCallbacks {
        on_agent_wait: std::sync::Arc::new({
            let append_status = std::sync::Arc::clone(&append_status);
            let ctx = ctx.clone();
            let message = message.clone();
            move || {
                let _ = append_status(ctx.clone(), &message, None, None);
            }
        }),
        on_review_scheduled: std::sync::Arc::new({
            let append_status = std::sync::Arc::clone(&append_status);
            let ctx = ctx.clone();
            move |review_at: &str| {
                let _ = append_status(ctx.clone(), &message, Some(review_at), None);
            }
        }),
    }
}
