//! Shared observable fetchWithTimeout/castToError behavior of the pinned Stainless SDKs.
use crate::request::{CallbackError, Fetch, HttpRequest, HttpResponse};
use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal};
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::Arc;
use std::time::Duration;

pub(crate) const DEFAULT_TIMEOUT_MS: f64 = 600_000.0;

pub(crate) async fn fetch_with_timeout(
    fetch: Arc<dyn Fetch>,
    request: HttpRequest,
    caller: Option<&AbortSignal>,
    timeout_ms: f64,
) -> Result<HttpResponse, CallbackError> {
    let controller = AbortController::new();
    if let Some(caller) = caller {
        let target = controller.clone();
        caller.add_listener(move |_| target.abort(None));
    }
    let future = fetch.fetch(request, Some(controller.signal()));
    tokio::pin!(future);
    let delay = pi_js_compat::node_timers::normalize_timeout_delay(timeout_ms);
    tokio::select! {
        result = &mut future => result,
        () = tokio::time::sleep(Duration::from_secs_f64(delay / 1000.0)) => {
            controller.abort(None);
            future.await
        }
    }
}

fn cast_to_error(error: &CallbackError) -> pi_js_compat::node_error::NativeNodeError {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => error,
        AbortReason::Value(value) => {
            let message = match value {
                JsValue::Object(_) | JsValue::Array(_) => pi_js_compat::js_json::stringify_lossless(&value).into(),
                _ => pi_js_compat::js_json::to_js_string(&value).expect("primitive String conversion"),
            };
            pi_js_compat::node_error::NativeNodeError::new("Error", message)
        }
    }
}

pub(crate) fn is_connection_timeout(error: &CallbackError) -> Result<bool, CallbackError> {
    let error = cast_to_error(error);
    if error.name == "AbortError" || error.message.contains("FetchRequestCanceledException") {
        return Ok(true);
    }
    let description = format_error_description(&error)?;
    Ok(matches_timeout_text(&description))
}

fn format_error_description(error: &pi_js_compat::node_error::NativeNodeError) -> Result<JsString, CallbackError> {
    let mut description = if error.name.is_empty() {
        error.message.clone()
    } else if error.message.is_empty() {
        error.name.clone()
    } else {
        JsString::join([error.name.clone(), ": ".into(), error.message.clone()], "")
    };
    if let Some(cause) = error.value().get("cause") {
        let cause = pi_js_compat::js_json::to_js_string(cause).map_err(|message| {
            CallbackError::from_rejection(AbortReason::Error(pi_js_compat::node_error::NativeNodeError::new(
                "TypeError",
                message,
            )))
        })?;
        description = JsString::join([description, cause], "");
    }
    Ok(description)
}

fn matches_timeout_text(description: &JsString) -> bool {
    // SDK regex is ASCII /timed? ?out/i; native units outside ASCII remain untouched.
    let units: Vec<u16> = description
        .units()
        .iter()
        .map(|unit| if (65..=90).contains(unit) { unit + 32 } else { *unit })
        .collect();
    ["timeout", "time out", "timedout", "timed out"].iter().any(|pattern| {
        let pattern: Vec<u16> = pattern.encode_utf16().collect();
        units.windows(pattern.len()).any(|window| window == pattern)
    })
}
