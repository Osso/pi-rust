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

fn supervisor_review_error_reason(error: &str) -> String {
    let trimmed = pi_js_compat::string::js_trim(error);
    if !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    "Unknown Supervisor review error".to_owned()
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
            let _ = append_status(input.ctx.clone(), &WAITING_FOR_SUPERVISOR_STATUS.into(), None);
            match review_goal(input).await {
                Ok(response) => Ok(response),
                Err(error) => Ok(GoalSupervisorResponse::Error {
                    reason: supervisor_review_error_reason(&error.to_string()).into(),
                }),
            }
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
            | SupervisorResponseKind::Error { .. } => Ok(response),
            _ => Ok(GoalSupervisorResponse::Error {
                reason: format!("Invalid goal review response: {}", response.kind_name()).into(),
            }),
        }
    })
}
