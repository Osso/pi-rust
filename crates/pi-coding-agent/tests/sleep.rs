use std::time::{Duration, Instant};

use pi_coding_agent::utils::sleep::{SleepError, sleep};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn sleeps_for_the_requested_duration_without_a_signal() {
    let started = Instant::now();
    sleep(30, None).await.expect("sleep without a signal");
    assert!(started.elapsed() >= Duration::from_millis(25));
}

#[tokio::test]
async fn rejects_immediately_when_already_aborted() {
    let token = CancellationToken::new();
    token.cancel();

    let started = Instant::now();
    let error = sleep(5_000, Some(&token)).await.expect_err("aborted sleep");
    assert_eq!(error, SleepError::Aborted);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn aborts_a_pending_sleep() {
    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        canceller.cancel();
    });

    let started = Instant::now();
    let error = sleep(5_000, Some(&token)).await.expect_err("aborted sleep");
    assert_eq!(error, SleepError::Aborted);
    assert!(started.elapsed() < Duration::from_secs(1));
}
