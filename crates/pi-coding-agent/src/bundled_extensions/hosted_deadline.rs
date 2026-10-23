//! Shared setTimeout/AbortSignal.any sequence in the two Codex hosted-tool modules.
use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal};
use pi_js_compat::js_json::JsString;
use pi_js_compat::node_error::NativeNodeError;

pub(super) fn format_timeout(timeout_ms: u64) -> String {
    if timeout_ms.is_multiple_of(1000) {
        format!("{}s", timeout_ms / 1000)
    } else {
        format!("{timeout_ms}ms")
    }
}

pub(super) fn spawn_deadline(
    caller: Option<AbortSignal>,
    timeout_ms: u64,
    message: JsString,
) -> (AbortSignal, tokio::task::JoinHandle<()>) {
    let controller = AbortController::new();
    let signal = match caller {
        Some(caller) => AbortSignal::any(&[caller, controller.signal()]),
        None => controller.signal(),
    };
    let delay = pi_js_compat::node_timers::normalize_timeout_delay(timeout_ms as f64);
    let sleep = tokio::time::sleep(std::time::Duration::from_secs_f64(delay / 1000.0));
    let timer = tokio::spawn(async move {
        sleep.await;
        controller.abort(Some(AbortReason::Error(NativeNodeError::new("Error", message))));
    });
    (signal, timer)
}
