//! Supervisor request client (`supervisor/client.ts`).

use std::time::{SystemTime, UNIX_EPOCH};

use super::ensure_running::{
    EnsureSupervisorRunningDependencies, EnsureSupervisorRunningInput, ensure_supervisor_running,
};
use pi_ai::request::BoxFuture;
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::core::session_control_db::{
    ControlDbError, PostSupervisorRequestInput, SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse,
    cancel_supervisor_request, post_supervisor_request, read_supervisor_request,
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
    pub signal: Option<CancellationToken>,
}

pub const SUPERVISOR_REQUEST_CANCELLED_REASON: &str = "Supervisor request cancelled";
pub const DEFAULT_SUPERVISOR_POLL_INTERVAL_MS: u64 = 300;

type EnsureRunning = Arc<dyn Fn(String) -> BoxFuture<'static, Result<(), String>> + Send + Sync>;
#[derive(Clone)]
pub struct SupervisorClientDependencies {
    pub ensure_running: EnsureRunning,
}
impl Default for SupervisorClientDependencies {
    fn default() -> Self {
        Self {
            ensure_running: Arc::new(|path| {
                Box::pin(async move {
                    ensure_supervisor_running(
                        &EnsureSupervisorRunningInput::new(path),
                        &EnsureSupervisorRunningDependencies::default(),
                    )
                    .await
                    .map(|_| ())
                })
            }),
        }
    }
}
fn cancelled_response() -> SupervisorResponse {
    SupervisorResponse::Error {
        reason: SUPERVISOR_REQUEST_CANCELLED_REASON.into(),
    }
}
fn is_aborted(input: &RequestSupervisorDecisionInput) -> bool {
    input.signal.as_ref().is_some_and(CancellationToken::is_cancelled)
}
fn cancel_request(input: &RequestSupervisorDecisionInput, id: i64) -> Result<(), ControlDbError> {
    if cancel_supervisor_request(
        &input.control_db_path,
        id,
        &input.sender_session_id,
        SUPERVISOR_REQUEST_CANCELLED_REASON,
    )? {
        notify_supervisor_request(&input.control_db_path);
    }
    Ok(())
}
async fn delay(milliseconds: u64, signal: Option<&CancellationToken>) -> bool {
    let sleep = tokio::time::sleep(std::time::Duration::from_millis(milliseconds));
    match signal {
        Some(signal) => tokio::select! {biased; ()=signal.cancelled()=>false, ()=sleep=>true},
        None => {
            sleep.await;
            true
        }
    }
}
async fn request_supervisor_decision_attempt(
    input: &RequestSupervisorDecisionInput,
    dependencies: &SupervisorClientDependencies,
) -> Result<Option<SupervisorResponse>, ControlDbError> {
    if is_aborted(input) {
        return Ok(Some(cancelled_response()));
    }
    if let Err(error) = (dependencies.ensure_running)(input.control_db_path.clone()).await {
        return Ok(Some(SupervisorResponse::Error {
            reason: format!("Supervisor startup failed: {error}"),
        }));
    }
    let deadline = now_millis() + input.timeout_ms;
    if is_aborted(input) {
        return Ok(Some(cancelled_response()));
    }
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
    let poll_interval_ms = input.poll_interval_ms.unwrap_or(DEFAULT_SUPERVISOR_POLL_INTERVAL_MS);
    while now_millis() < deadline {
        if is_aborted(input) {
            cancel_request(input, request_id)?;
            return Ok(Some(cancelled_response()));
        }
        if let Some(request) = read_supervisor_request(&input.control_db_path, request_id)? {
            if request.status == SupervisorRequestStatus::Completed && request.response.is_some() {
                return Ok(request.response);
            }
            if request.status == SupervisorRequestStatus::Cancelled {
                let response = match request.response {
                    Some(response @ SupervisorResponse::Error { .. }) => response,
                    _ => cancelled_response(),
                };
                return Ok(Some(response));
            }
        }
        if !delay(poll_interval_ms, input.signal.as_ref()).await {
            cancel_request(input, request_id)?;
            return Ok(Some(cancelled_response()));
        }
    }
    Ok(None)
}

/// `baseDelay * 2 ** (attempt - 1) * (1 - jitter + Math.random() * jitter * 2)`.
fn retry_delay(input: &RequestSupervisorDecisionInput, attempt: u32) -> u64 {
    let base_delay_ms = input.retry_delay_ms.unwrap_or(1_000.0);
    let jitter_ratio = input.retry_jitter_ratio.unwrap_or(0.2);
    let exponential_delay_ms = base_delay_ms * 2f64.powi(i32::try_from(attempt.saturating_sub(1)).unwrap_or(0));
    let delay = exponential_delay_ms * (1.0 - jitter_ratio + crate::utils::random::random_f64() * jitter_ratio * 2.0);
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
    request_supervisor_decision_with_dependencies(input, &SupervisorClientDependencies::default()).await
}

pub async fn request_supervisor_decision_with_dependencies(
    input: &RequestSupervisorDecisionInput,
    dependencies: &SupervisorClientDependencies,
) -> Result<SupervisorResponse, ControlDbError> {
    let max_attempts = input.max_attempts.unwrap_or(1);
    for attempt in 1..=max_attempts {
        if let Some(response) = request_supervisor_decision_attempt(input, dependencies).await? {
            return Ok(response);
        }
        if attempt < max_attempts && !delay(retry_delay(input, attempt), input.signal.as_ref()).await {
            return Ok(cancelled_response());
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
