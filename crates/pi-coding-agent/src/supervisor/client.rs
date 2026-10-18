//! Port of `packages/coding-agent/src/supervisor/client.ts`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::session_control_db::{
    ControlDbError, PostSupervisorRequestInput, SupervisorRequestKind, SupervisorResponse, post_supervisor_request,
    read_supervisor_request,
};
use crate::supervisor::request_wake::notify_supervisor_request;
use crate::utils::iso_time;
use serde_json::Map;

/// Inputs for one durable Supervisor decision request.
#[derive(Clone)]
pub struct RequestSupervisorDecisionInput {
    pub control_db_path: String,
    pub kind: SupervisorRequestKind,
    pub payload: Map<String, serde_json::Value>,
    pub project_id: String,
    pub sender_session_id: String,
    pub timeout_ms: u64,
    pub max_attempts: Option<u32>,
    pub poll_interval_ms: Option<u64>,
    pub retry_delay_ms: Option<u64>,
    pub retry_jitter_ratio: Option<f64>,
}

/// `requestSupervisorDecision`.
pub async fn request_supervisor_decision(
    input: &RequestSupervisorDecisionInput,
) -> Result<SupervisorResponse, ControlDbError> {
    let max_attempts = input.max_attempts.unwrap_or(1);
    for attempt in 1..=max_attempts {
        if let Some(response) = request_supervisor_decision_attempt(input).await? {
            return Ok(response);
        }
        if attempt < max_attempts {
            tokio::time::sleep(Duration::from_millis(retry_delay(input, attempt))).await;
        }
    }
    let suffix = if max_attempts == 1 {
        String::new()
    } else {
        format!(" after {max_attempts} attempts")
    };
    Ok(SupervisorResponse::Error {
        reason: format!("Supervisor request timed out{suffix}"),
    })
}

async fn request_supervisor_decision_attempt(
    input: &RequestSupervisorDecisionInput,
) -> Result<Option<SupervisorResponse>, ControlDbError> {
    let deadline = now_millis() + input.timeout_ms as i64;
    let request_id = post_supervisor_request(
        &input.control_db_path,
        &PostSupervisorRequestInput {
            sender_session_id: input.sender_session_id.clone(),
            project_id: input.project_id.clone(),
            kind: input.kind,
            payload: input.payload.clone(),
            deadline_at: iso_time::format_iso_millis(deadline),
        },
    )?;
    notify_supervisor_request(&input.control_db_path);
    let poll_interval_ms = input.poll_interval_ms.unwrap_or(50);
    while now_millis() < deadline {
        let request = read_supervisor_request(&input.control_db_path, request_id)?;
        if let Some(request) = request
            && request.status == crate::core::session_control_db::SupervisorRequestStatus::Completed
            && let Some(response) = request.response
        {
            return Ok(Some(response));
        }
        tokio::time::sleep(Duration::from_millis(poll_interval_ms)).await;
    }
    Ok(None)
}

fn retry_delay(input: &RequestSupervisorDecisionInput, attempt: u32) -> u64 {
    let base_delay_ms = input.retry_delay_ms.unwrap_or(1_000) as f64;
    let jitter_ratio = input.retry_jitter_ratio.unwrap_or(0.2);
    let exponential_delay_ms = base_delay_ms * 2f64.powi(attempt.saturating_sub(1) as i32);
    (exponential_delay_ms * (1.0 - jitter_ratio + random_unit() * jitter_ratio * 2.0)).max(0.0) as u64
}

fn random_unit() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| f64::from(elapsed.subsec_nanos()) / 1_000_000_000.0)
        .unwrap_or(0.0)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
