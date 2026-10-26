//! Supervisor request client (`supervisor/client.ts`).
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_ai::request::{BoxFuture, now_millis};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use pi_js_compat::{js_format, node_timers::normalize_timeout_delay, number::js_number_to_string};

use crate::core::session_control_db::{
    ControlDbError, PostSupervisorRequestInput, SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse,
    cancel_supervisor_request, post_supervisor_request, read_supervisor_request,
};
use crate::supervisor::ensure_running::{EnsureSupervisorRunningInput, ensure_supervisor_running};
use crate::supervisor::request_wake::notify_supervisor_request;

pub const SUPERVISOR_REQUEST_CANCELLED_REASON: &str = "Supervisor request cancelled";
pub const DEFAULT_SUPERVISOR_POLL_INTERVAL_MS: f64 = 300.0;

#[derive(Debug, Clone)]
pub struct RequestSupervisorDecisionInput {
    pub control_db_path: JsString,
    pub kind: SupervisorRequestKind,
    pub payload: JsObject,
    pub project_id: JsString,
    pub sender_session_id: JsString,
    pub timeout_ms: f64,
    pub signal: Option<AbortSignal>,
    pub max_attempts: Option<f64>,
    pub poll_interval_ms: Option<f64>,
    pub retry_delay_ms: Option<f64>,
    pub retry_jitter_ratio: Option<f64>,
}
pub type EnsureSupervisorRunning =
    Arc<dyn Fn(JsString) -> BoxFuture<'static, Result<(), NativeNodeError>> + Send + Sync>;
#[derive(Clone, Default)]
pub struct SupervisorClientDependencies {
    pub ensure_running: Option<EnsureSupervisorRunning>,
}

fn database_error(error: ControlDbError) -> NativeNodeError {
    NativeNodeError::new("Error", error.message().clone())
}
fn project_supervisor_response(response: &JsValue) -> Result<SupervisorResponse, NativeNodeError> {
    pi_js_compat::js_json::from_value(response).map_err(|error| NativeNodeError::new("Error", error.to_string()))
}
fn cancelled() -> SupervisorResponse {
    SupervisorResponse::Error {
        reason: SUPERVISOR_REQUEST_CANCELLED_REASON.into(),
    }
}
fn aborted(signal: Option<&AbortSignal>) -> bool {
    signal.is_some_and(AbortSignal::aborted)
}
fn cancel_request(input: &RequestSupervisorDecisionInput, request_id: i64) -> Result<(), NativeNodeError> {
    if cancel_supervisor_request(
        &input.control_db_path,
        request_id,
        &input.sender_session_id,
        SUPERVISOR_REQUEST_CANCELLED_REASON,
    )
    .map_err(database_error)?
    {
        notify_supervisor_request(&input.control_db_path);
    }
    Ok(())
}
async fn request_supervisor_decision_attempt(
    input: &RequestSupervisorDecisionInput,
    ensure_running: &EnsureSupervisorRunning,
) -> Result<Option<SupervisorResponse>, NativeNodeError> {
    if aborted(input.signal.as_ref()) {
        return Ok(Some(cancelled()));
    }
    if let Err(error) = ensure_running(input.control_db_path.clone()).await {
        return Ok(Some(SupervisorResponse::Error {
            reason: js_format!("Supervisor startup failed: {}", error.message),
        }));
    }
    let deadline = now_millis() as f64 + input.timeout_ms;
    if aborted(input.signal.as_ref()) {
        return Ok(Some(cancelled()));
    }
    let deadline_at = pi_js_compat::date::to_iso_string_value(deadline)
        .map_err(|_| NativeNodeError::from_constructor(NativeErrorClass::RangeError, "Invalid time value"))?;
    let request_id = post_supervisor_request(
        &input.control_db_path,
        &PostSupervisorRequestInput {
            deadline_at,
            kind: input.kind,
            payload: input.payload.clone(),
            project_id: input.project_id.clone(),
            sender_session_id: input.sender_session_id.clone(),
        },
    )
    .map_err(database_error)?;
    let listener_error = Arc::new(Mutex::new(None));
    let _subscription = if let Some(signal) = &input.signal {
        let listener_input = input.clone();
        let error_slot = listener_error.clone();
        match signal.subscribe(move |_| {
            if let Err(error) = cancel_request(&listener_input, request_id) {
                *error_slot.lock().expect("cancellation error slot") = Some(error);
            }
        }) {
            Ok(subscription) => Some(subscription),
            Err(_) => {
                cancel_request(input, request_id)?;
                None
            }
        }
    } else {
        None
    };
    notify_supervisor_request(&input.control_db_path);
    while (now_millis() as f64) < deadline {
        if let Some(error) = listener_error.lock().expect("cancellation error slot").take() {
            return Err(error);
        }
        if aborted(input.signal.as_ref()) {
            cancel_request(input, request_id)?;
            return Ok(Some(cancelled()));
        }
        if let Some(request) = read_supervisor_request(&input.control_db_path, request_id).map_err(database_error)? {
            if request.status == SupervisorRequestStatus::Completed
                && let Some(response) = request.response
            {
                return Ok(Some(project_supervisor_response(&response)?));
            }
            if request.status == SupervisorRequestStatus::Cancelled {
                let response = request.response.as_ref().map(project_supervisor_response).transpose()?;
                return Ok(Some(match response {
                    Some(SupervisorResponse::Error { reason }) => SupervisorResponse::Error { reason },
                    _ => cancelled(),
                }));
            }
        }
        if !delay(
            input.poll_interval_ms.unwrap_or(DEFAULT_SUPERVISOR_POLL_INTERVAL_MS),
            input.signal.as_ref(),
        )
        .await
        {
            if let Some(error) = listener_error.lock().expect("cancellation error slot").take() {
                return Err(error);
            }
            cancel_request(input, request_id)?;
            return Ok(Some(cancelled()));
        }
    }
    Ok(None)
}

/// `baseDelay * 2 ** (attempt - 1) * (1 - jitter + Math.random() * jitter * 2)`.
fn retry_delay(input: &RequestSupervisorDecisionInput, attempt: f64) -> f64 {
    let base = input.retry_delay_ms.unwrap_or(1000.0);
    let jitter = input.retry_jitter_ratio.unwrap_or(0.2);
    base * 2f64.powf(attempt - 1.0) * (1.0 - jitter + crate::utils::random::random_f64() * jitter * 2.0)
}

pub async fn request_supervisor_decision(
    input: &RequestSupervisorDecisionInput,
) -> Result<SupervisorResponse, NativeNodeError> {
    request_supervisor_decision_with_dependencies(input, &SupervisorClientDependencies::default()).await
}
/// Frozen optional dependency injection; default uses the whole startup coordinator.
pub async fn request_supervisor_decision_with_dependencies(
    input: &RequestSupervisorDecisionInput,
    dependencies: &SupervisorClientDependencies,
) -> Result<SupervisorResponse, NativeNodeError> {
    let ensure_running = dependencies.ensure_running.clone().unwrap_or_else(|| {
        Arc::new(|path| {
            Box::pin(async move {
                ensure_supervisor_running(&EnsureSupervisorRunningInput::new(path), &Default::default())
                    .await
                    .map(|_| ())
            })
        })
    });
    let max_attempts = input.max_attempts.unwrap_or(1.0);
    let mut attempt = 1.0;
    while attempt <= max_attempts {
        if let Some(response) = request_supervisor_decision_attempt(input, &ensure_running).await? {
            return Ok(response);
        }
        if attempt < max_attempts && !delay(retry_delay(input, attempt), input.signal.as_ref()).await {
            return Ok(cancelled());
        }
        attempt += 1.0;
    }
    let suffix = if max_attempts == 1.0 {
        JsString::default()
    } else {
        js_format!(" after {} attempts", js_number_to_string(max_attempts))
    };
    Ok(SupervisorResponse::Error {
        reason: js_format!("Supervisor request timed out{}", suffix),
    })
}
async fn delay(milliseconds: f64, signal: Option<&AbortSignal>) -> bool {
    if aborted(signal) {
        return false;
    }
    let timer = tokio::time::sleep(Duration::from_millis(
        normalize_timeout_delay(milliseconds).trunc() as u64
    ));
    if let Some(signal) = signal {
        tokio::select! {biased; _=signal.wait()=>false, ()=timer=>true}
    } else {
        timer.await;
        true
    }
}

/// `payload` object key order follows the caller's insertion order.
pub fn advisory_payload(question: &JsString, context: Option<&JsString>) -> JsObject {
    let mut payload = JsObject::new();
    payload.insert("question".into(), JsValue::String(question.clone()));
    if let Some(context) = context {
        payload.insert("context".into(), JsValue::String(context.clone()));
    }
    payload
}
