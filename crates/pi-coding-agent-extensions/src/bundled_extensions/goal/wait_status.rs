//! Port of `packages/coding-agent/extensions/goal/src/wait-status.ts`.

use super::goal_scheduling::GoalWaitCallbacks;
use super::rendering::AppendSupervisorStatus;
use crate::core::extensions::types::{ExtensionContext, HandlerError};
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

pub fn append_goal_scheduling_error(
    append_status: &AppendSupervisorStatus,
    error: &JsString,
    ctx: ExtensionContext,
) -> Result<(), HandlerError> {
    append_status(ctx, &(JsString::from("Goal wait failed: ") + error), None, None)
}

pub fn create_wait_status_callbacks(
    append_status: AppendSupervisorStatus,
    ctx: ExtensionContext,
    message: JsString,
    on_agent_wake: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
) -> GoalWaitCallbacks {
    let on_agent_wait = {
        let append_status = append_status.clone();
        let ctx = ctx.clone();
        let message = message.clone();
        Arc::new(move |review_at: &str| append_status(ctx.clone(), &message, Some(review_at), None))
    };
    GoalWaitCallbacks {
        on_agent_wait,
        on_agent_wake,
        on_review_scheduled: Arc::new(move |review_at| append_status(ctx.clone(), &message, Some(review_at), None)),
    }
}
