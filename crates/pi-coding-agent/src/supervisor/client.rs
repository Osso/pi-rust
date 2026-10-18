//! Supervisor request client (`supervisor/client.ts`).

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::core::session_control_db::{
    ControlDbError, PostSupervisorRequestInput, SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse,
    post_supervisor_request, read_supervisor_request,
};
use crate::supervisor::request_wake::notify_supervisor_request;
use crate::utils::iso_time::format_iso_millis;

#[derive(Debug, Clone)]
pub struct RequestSupervisorDecisionInput {
    pub control_db_path: String,
    pub kind: SupervisorRequestKind,
    pub payload: Map<String, Value>,
    pub project_id: String,
    pub sender_session_id: String,
    pub timeout_ms: i64,
    pub max_attempts: Option<u32>,
    pub poll_interval_ms: Option<u64>,
    pub retry_delay_ms: Option<f64>,
    pub retry_jitter_ratio: Option<f64>,
}

async fn request_supervisor_decision_attempt(
    input: &RequestSupervisorDecisionInput,
) -> Result<Option<SupervisorResponse>, ControlDbError> {
    let deadline = now_millis() + input.timeout_ms;
    let request_id = post_supervisor_request(
        &input.control_db_path,
        &PostSupervisorRequestInput {
            deadline_at: format_iso_millis(deadline),
            kind: input.kind,
            payload: input.payload.clone(),
            project_id: input.project_id.clone(),
            sender_session_id: input.sender_session_id.clone(),
        },
    )?;
    notify_supervisor_request(&input.control_db_path);
    let poll_interval_ms = input.poll_interval_ms.unwrap_or(50);
    while now_millis() < deadline {
        let request = read_supervisor_request(&input.control_db_path, request_id)?;
        if let Some(request) = request {
            if request.status == SupervisorRequestStatus::Completed {
                if let Some(response) = request.response {
                    return Ok(Some(response));
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(poll_interval_ms)).await;
    }
    Ok(None)
}

/// `baseDelay * 2 ** (attempt - 1) * (1 - jitter + Math.random() * jitter * 2)`.
fn retry_delay(input: &RequestSupervisorDecisionInput, attempt: u32) -> u64 {
    let base_delay_ms = input.retry_delay_ms.unwrap_or(1_000.0);
    let jitter_ratio = input.retry_jitter_ratio.unwrap_or(0.2);
    let exponential_delay_ms = base_delay_ms * 2f64.powi(i32::try_from(attempt.saturating_sub(1)).unwrap_or(0));
    let delay = exponential_delay_ms * (1.0 - jitter_ratio + random_f64() * jitter_ratio * 2.0);
    if delay.is_finite() && delay > 0.0 {
        delay as u64
    } else {
        0
    }
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
            tokio::time::sleep(std::time::Duration::from_millis(retry_delay(input, attempt))).await;
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

/// `payload` object key order follows the caller's insertion order.
pub fn advisory_payload(question: &str, context: Option<&str>) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.insert("question".to_owned(), Value::String(question.to_owned()));
    if let Some(context) = context {
        payload.insert("context".to_owned(), Value::String(context.to_owned()));
    }
    payload
}

fn now_millis() -> i64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch");
    i64::try_from(elapsed.as_millis()).expect("timestamp fits in i64")
}

fn random_f64() -> f64 {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS random source must be available");
    let value = u64::from_le_bytes(bytes) >> 11;
    value as f64 / (1u64 << 53) as f64
}
