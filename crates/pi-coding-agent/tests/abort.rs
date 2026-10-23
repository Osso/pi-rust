use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use pi_coding_agent::utils::abort::{AbortError, operation_signal, race_with_abort_signal};

#[tokio::test]
async fn returns_the_operation_value_without_a_signal() {
    let value = race_with_abort_signal(async { 42 }, None)
        .await
        .expect("operation completes");
    assert_eq!(value, 42);
}

#[tokio::test]
async fn rejects_an_already_aborted_operation_without_polling_it() {
    let controller_token = pi_js_compat::abort::AbortController::new();
    let token = controller_token.signal();
    controller_token.abort(None);
    let polled = Arc::new(AtomicBool::new(false));
    let polled_by_operation = polled.clone();

    let error = race_with_abort_signal(
        async move {
            polled_by_operation.store(true, Ordering::SeqCst);
            1
        },
        Some(&token),
    )
    .await
    .expect_err("pre-aborted operation");

    assert_eq!(error, AbortError::new("The operation was aborted"));
    assert!(!polled.load(Ordering::SeqCst), "aborted future must not start");
}

#[tokio::test]
async fn rejects_a_pending_operation_when_aborted() {
    let controller_token = pi_js_compat::abort::AbortController::new();
    let token = controller_token.signal();
    let controller_canceller = controller_token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        controller_canceller.abort(None);
    });

    let started = Instant::now();
    let error = race_with_abort_signal(
        async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            1
        },
        Some(&token),
    )
    .await
    .expect_err("aborted operation");

    assert_eq!(error, AbortError::new("The operation was aborted"));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn propagates_operation_errors() {
    let result = race_with_abort_signal(async { Err::<(), &str>("boom") }, None).await;
    assert_eq!(result.expect("operation settles"), Err("boom"));
}

#[tokio::test]
async fn operation_signal_returns_the_supplied_token_or_a_fresh_one() {
    let controller_supplied = pi_js_compat::abort::AbortController::new();
    let supplied = controller_supplied.signal();
    let returned = operation_signal(Some(supplied.clone()));
    assert_eq!(returned, supplied);

    let fresh = operation_signal(None);
    assert!(!fresh.aborted());
}
