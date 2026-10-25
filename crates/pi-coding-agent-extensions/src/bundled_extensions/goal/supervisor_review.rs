//! Port of `packages/coding-agent/extensions/goal/src/supervisor-review.ts`.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use crate::config::get_user_state_root;
use crate::core::extensions::types::{BoxFuture, HandlerError};
use crate::core::session_control_db::{
    SupervisorRequestKind, SupervisorResponse as SupervisorResponseKind, get_control_db_path,
};
use crate::supervisor::client::{RequestSupervisorDecisionInput, request_supervisor_decision};
use crate::supervisor::project_resolver::{DEFAULT_SUPERVISOR_KB_DIR, resolve_supervisor_project_for_cwd};

use super::goal_types::{GoalReviewInput, GoalReviewKind, GoalSupervisorResponse, GoalSupervisorReview};
use super::rendering::AppendSupervisorStatus;

const GOAL_REVIEW_MAX_ATTEMPTS: u32 = 3;
const GOAL_REVIEW_RETRY_DELAY_MS: f64 = 1_000.0;
const GOAL_REVIEW_TIMEOUT_MS: i64 = 60_000;
const WAITING_FOR_SUPERVISOR_STATUS: &str = "Waiting for Supervisor…";

fn supervisor_review_error_reason(error: &HandlerError) -> JsString {
    if !error.message.trim().is_empty() {
        return error.message.clone();
    }
    let reason = (JsString::from("Error: ") + &error.message).trim();
    if error.message.is_empty() {
        return "Error".into();
    }
    if reason.is_empty() {
        "Unknown Supervisor review error".into()
    } else {
        reason
    }
}

fn review_result_message(result: &GoalSupervisorResponse) -> JsString {
    match result {
        GoalSupervisorResponse::Continue { instructions, .. } => instructions.clone(),
        GoalSupervisorResponse::Complete { reason, .. } => JsString::from("Goal complete: ") + reason,
        GoalSupervisorResponse::Set { objective, .. } => JsString::from("Goal set: ") + objective,
        GoalSupervisorResponse::Pause { reason, .. } => JsString::from("Goal waiting: ") + reason,
        GoalSupervisorResponse::Wait { reason, .. } => JsString::from("Waiting: ") + reason,
        GoalSupervisorResponse::Error { reason, .. }
            if reason == crate::supervisor::client::SUPERVISOR_REQUEST_CANCELLED_REASON =>
        {
            "Supervisor review cancelled.".into()
        }
        GoalSupervisorResponse::Error { reason, .. } => JsString::from("Goal review failed: ") + reason,
    }
}

/// `withSupervisorReviewStatus`.
pub fn with_supervisor_review_status(
    append_status: AppendSupervisorStatus,
    review_goal: GoalSupervisorReview,
) -> GoalSupervisorReview {
    Arc::new(move |input: GoalReviewInput| {
        let append_status = Arc::clone(&append_status);
        let review_goal = Arc::clone(&review_goal);
        Box::pin(async move {
            let display_id: JsString = pi_ai::utils::random_uuid::random_uuid()
                .map_err(HandlerError::from)?
                .into();
            let ctx = input.ctx.clone();
            append_status(
                ctx.clone(),
                &WAITING_FOR_SUPERVISOR_STATUS.into(),
                None,
                Some(&display_id),
            )?;
            let mut result = match review_goal(input).await {
                Ok(response) => response,
                Err(error) => GoalSupervisorResponse::Error {
                    reason: supervisor_review_error_reason(&error),
                    display_id: None,
                },
            };
            append_status(ctx, &review_result_message(&result), None, Some(&display_id))?;
            result.set_display_id(display_id);
            Ok(result)
        }) as BoxFuture<'static, Result<GoalSupervisorResponse, HandlerError>>
    })
}

fn request_kind(kind: GoalReviewKind) -> SupervisorRequestKind {
    match kind {
        GoalReviewKind::GoalCompletionReview => SupervisorRequestKind::GoalCompletionReview,
        GoalReviewKind::GoalIdleReview => SupervisorRequestKind::GoalIdleReview,
        GoalReviewKind::GoalSetReview => SupervisorRequestKind::GoalSetReview,
    }
}

/// `reviewGoalWithResidentSupervisor`.
pub fn review_goal_with_resident_supervisor(
    input: GoalReviewInput,
) -> BoxFuture<'static, Result<GoalSupervisorResponse, HandlerError>> {
    Box::pin(async move {
        if input.ctx.no_supervisor()? {
            return Err(HandlerError::new(
                "Supervisor goal review is unavailable in standalone worker mode",
            ));
        }
        let kb_dir = JsString::from(
            pi_js_compat::process_env::get("PI_KB_DIR").unwrap_or_else(|| DEFAULT_SUPERVISOR_KB_DIR.to_owned()),
        );
        let control_db_path = JsString::from(get_control_db_path(&get_user_state_root().map_err(HandlerError::from)?));
        let cwd = input.ctx.cwd()?;
        let session = input.ctx.session_manager()?;
        let sender_session_id = session.get_session_id().clone();
        let project_id = resolve_supervisor_project_for_cwd(&cwd, &kb_dir).map_err(HandlerError::new)?;
        let response = request_supervisor_decision(&RequestSupervisorDecisionInput {
            control_db_path,
            kind: request_kind(input.kind),
            payload: input.payload,
            project_id,
            sender_session_id,
            timeout_ms: GOAL_REVIEW_TIMEOUT_MS as f64,
            signal: None,
            max_attempts: Some(f64::from(GOAL_REVIEW_MAX_ATTEMPTS)),
            poll_interval_ms: None,
            retry_delay_ms: Some(GOAL_REVIEW_RETRY_DELAY_MS),
            retry_jitter_ratio: None,
        })
        .await
        .map_err(|error| HandlerError::new(error.to_string()))?;
        match response {
            SupervisorResponseKind::Complete { .. }
            | SupervisorResponseKind::Continue { .. }
            | SupervisorResponseKind::Pause { .. }
            | SupervisorResponseKind::Wait { .. }
            | SupervisorResponseKind::Set { .. }
            | SupervisorResponseKind::Error { .. } => {
                let value =
                    pi_js_compat::js_json::to_value(&response).map_err(|error| HandlerError::new(error.to_string()))?;
                pi_js_compat::js_json::from_value(&value).map_err(|error| HandlerError::new(error.to_string()))
            }
            _ => Ok(GoalSupervisorResponse::Error {
                reason: format!("Invalid goal review response: {}", response.kind_name()).into(),
                display_id: None,
            }),
        }
    })
}
