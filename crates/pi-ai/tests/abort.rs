use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;

use pi_ai::utils::abort::{AbortError, operation_signal, race_with_abort_signal};
use pi_ai::utils::abort_signals::combine_abort_signals;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn races_an_operation_against_cancellation() {
    let signal = CancellationToken::new();
    let cancelled = signal.clone();
    let task = tokio::spawn(async move {
        tokio::task::yield_now().await;
        cancelled.cancel();
    });

    let error = race_with_abort_signal(std::future::pending::<()>(), &signal)
        .await
        .expect_err("pending operation must abort");
    assert_eq!(error, AbortError::new());
    assert_eq!(error.to_string(), "The operation was aborted");
    task.await.unwrap();
}

#[tokio::test]
async fn returns_the_operation_value_when_it_completes_first() {
    let signal = CancellationToken::new();
    let value = race_with_abort_signal(async { 42 }, &signal).await.unwrap();
    assert_eq!(value, 42);
}

#[tokio::test]
async fn rejects_immediately_without_polling_when_already_cancelled() {
    let signal = CancellationToken::new();
    signal.cancel();
    let polls = Arc::new(AtomicUsize::new(0));
    let operation_polls = Arc::clone(&polls);
    let operation = std::future::poll_fn(move |_| {
        operation_polls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(())
    });

    assert_eq!(race_with_abort_signal(operation, &signal).await, Err(AbortError::new()));
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn operation_signal_uses_the_given_token_or_a_fresh_one() {
    let fresh = operation_signal(None);
    assert!(!fresh.is_cancelled());

    let provided = CancellationToken::new();
    let shared = operation_signal(Some(&provided));
    provided.cancel();
    assert!(shared.is_cancelled());
}

#[tokio::test]
async fn combines_no_signals_without_a_token() {
    let combined = combine_abort_signals(&[]);
    assert!(combined.signal().is_none());
    combined.cleanup();
}

#[tokio::test]
async fn combines_one_signal_without_a_bridge_task() {
    let signal = CancellationToken::new();
    let combined = combine_abort_signals(&[Some(signal.clone())]);

    assert!(combined.signal().is_some());
    signal.cancel();
    assert!(combined.signal().unwrap().is_cancelled());
    combined.cleanup();
}

#[tokio::test]
async fn combines_multiple_signals_into_a_child_token() {
    let first = CancellationToken::new();
    let second = CancellationToken::new();
    let combined = combine_abort_signals(&[Some(first.clone()), None, Some(second.clone())]);
    let token = combined.signal().unwrap().clone();
    assert!(!token.is_cancelled());

    first.cancel();
    tokio::task::yield_now().await;
    assert!(token.is_cancelled());
    combined.cleanup();
}

#[tokio::test]
async fn stops_propagating_after_cleanup() {
    let first = CancellationToken::new();
    let second = CancellationToken::new();
    let combined = combine_abort_signals(&[Some(first.clone()), Some(second.clone())]);
    let token = combined.signal().unwrap().clone();

    combined.cleanup();
    second.cancel();
    tokio::task::yield_now().await;
    assert!(!token.is_cancelled());
}

#[tokio::test]
async fn aborts_immediately_when_a_source_signal_is_already_cancelled() {
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let combined = combine_abort_signals(&[Some(cancelled), Some(CancellationToken::new())]);

    assert!(combined.signal().unwrap().is_cancelled());
    combined.cleanup();
}
