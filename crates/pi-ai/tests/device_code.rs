use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use pi_ai::auth::oauth::device_code::{
    OAuthDeviceCodeError, OAuthDeviceCodePollOptions, OAuthDeviceCodePollResult, poll_oauth_device_code_flow,
};
use pi_js_compat::abort::AbortSignal;

fn options(interval_seconds: Option<f64>, expires_in_seconds: Option<f64>) -> OAuthDeviceCodePollOptions {
    OAuthDeviceCodePollOptions {
        interval_seconds,
        expires_in_seconds,
        wait_before_first_poll: false,
        signal: AbortSignal::default(),
    }
}

fn sequence<T: Clone + Send + 'static>(
    results: Vec<OAuthDeviceCodePollResult<T>>,
    calls: Arc<AtomicUsize>,
) -> impl FnMut() -> std::future::Ready<OAuthDeviceCodePollResult<T>> {
    let mut results = VecDeque::from(results);
    move || {
        calls.fetch_add(1, Ordering::SeqCst);
        std::future::ready(results.pop_front().expect("unexpected extra poll"))
    }
}

#[tokio::test(start_paused = true)]
async fn polls_immediately_and_returns_the_completed_value() {
    let calls = Arc::new(AtomicUsize::new(0));
    let poll = sequence(
        vec![
            OAuthDeviceCodePollResult::Pending,
            OAuthDeviceCodePollResult::Complete("token"),
        ],
        Arc::clone(&calls),
    );
    let task = tokio::spawn(poll_oauth_device_code_flow(options(Some(2.0), Some(30.0)), poll));

    tokio::time::advance(Duration::from_millis(0)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(1999)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(1)).await;

    assert_eq!(task.await.unwrap(), Ok("token"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn can_wait_before_the_first_poll() {
    let calls = Arc::new(AtomicUsize::new(0));
    let poll = sequence(vec![OAuthDeviceCodePollResult::Complete("token")], Arc::clone(&calls));
    let mut poll_options = options(Some(2.0), Some(30.0));
    poll_options.wait_before_first_poll = true;
    let task = tokio::spawn(poll_oauth_device_code_flow(poll_options, poll));

    tokio::time::advance(Duration::from_millis(1999)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    tokio::time::advance(Duration::from_millis(1)).await;

    assert_eq!(task.await.unwrap(), Ok("token"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn increases_the_interval_by_five_seconds_after_slow_down() {
    let calls = Arc::new(AtomicUsize::new(0));
    let poll = sequence(
        vec![
            OAuthDeviceCodePollResult::SlowDown { interval_seconds: None },
            OAuthDeviceCodePollResult::Complete("token"),
        ],
        Arc::clone(&calls),
    );
    let task = tokio::spawn(poll_oauth_device_code_flow(options(Some(2.0), Some(900.0)), poll));

    tokio::time::advance(Duration::from_millis(0)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(6999)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(1)).await;

    assert_eq!(task.await.unwrap(), Ok("token"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn honors_a_server_provided_slow_down_interval() {
    let calls = Arc::new(AtomicUsize::new(0));
    let poll = sequence(
        vec![
            OAuthDeviceCodePollResult::SlowDown {
                interval_seconds: Some(30.0),
            },
            OAuthDeviceCodePollResult::Complete("token"),
        ],
        Arc::clone(&calls),
    );
    let task = tokio::spawn(poll_oauth_device_code_flow(options(Some(2.0), Some(900.0)), poll));

    tokio::time::advance(Duration::from_millis(0)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(29999)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(1)).await;

    assert_eq!(task.await.unwrap(), Ok("token"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn cancels_an_in_flight_wait() {
    let calls = Arc::new(AtomicUsize::new(0));
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    let poll = sequence::<&str>(vec![OAuthDeviceCodePollResult::Pending], Arc::clone(&calls));
    let mut poll_options = options(Some(5.0), Some(30.0));
    poll_options.signal = signal.clone();
    let task = tokio::spawn(poll_oauth_device_code_flow(poll_options, poll));

    tokio::time::advance(Duration::from_millis(0)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    signal_controller.abort(None);

    assert_eq!(task.await.unwrap(), Err(OAuthDeviceCodeError::Cancelled));
}

#[tokio::test(start_paused = true)]
async fn times_out_without_slow_down_responses() {
    let calls = Arc::new(AtomicUsize::new(0));
    let poll = sequence::<&str>(
        vec![
            OAuthDeviceCodePollResult::Pending,
            OAuthDeviceCodePollResult::Pending,
            OAuthDeviceCodePollResult::Pending,
        ],
        Arc::clone(&calls),
    );
    let task = tokio::spawn(poll_oauth_device_code_flow(options(Some(5.0), Some(12.0)), poll));

    tokio::time::advance(Duration::from_secs(13)).await;

    assert_eq!(task.await.unwrap(), Err(OAuthDeviceCodeError::TimedOut));
}

#[tokio::test(start_paused = true)]
async fn reports_the_clock_drift_message_after_slow_down_timeout() {
    let calls = Arc::new(AtomicUsize::new(0));
    let poll = sequence::<&str>(
        vec![
            OAuthDeviceCodePollResult::SlowDown { interval_seconds: None },
            OAuthDeviceCodePollResult::Pending,
        ],
        Arc::clone(&calls),
    );
    let task = tokio::spawn(poll_oauth_device_code_flow(options(Some(1.0), Some(2.0)), poll));

    tokio::time::advance(Duration::from_secs(3)).await;

    assert_eq!(task.await.unwrap(), Err(OAuthDeviceCodeError::SlowDownTimedOut));
}

#[tokio::test(start_paused = true)]
async fn surfaces_a_failed_poll_message() {
    let calls = Arc::new(AtomicUsize::new(0));
    let poll = sequence::<&str>(
        vec![OAuthDeviceCodePollResult::Failed {
            message: ("access_denied".to_owned()).into(),
        }],
        Arc::clone(&calls),
    );

    let result = poll_oauth_device_code_flow(options(Some(5.0), Some(30.0)), poll).await;

    assert_eq!(
        result,
        Err(OAuthDeviceCodeError::Failed(("access_denied".to_owned()).into()))
    );
}

#[tokio::test(start_paused = true)]
async fn enforces_a_one_second_minimum_interval() {
    let calls = Arc::new(AtomicUsize::new(0));
    let poll = sequence(
        vec![
            OAuthDeviceCodePollResult::Pending,
            OAuthDeviceCodePollResult::Complete("token"),
        ],
        Arc::clone(&calls),
    );
    let task = tokio::spawn(poll_oauth_device_code_flow(options(Some(0.5), Some(30.0)), poll));

    tokio::time::advance(Duration::from_millis(999)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(1)).await;

    assert_eq!(task.await.unwrap(), Ok("token"));
}

#[test]
fn error_messages_match_the_source() {
    assert_eq!(OAuthDeviceCodeError::Cancelled.to_string(), "Login cancelled");
    assert_eq!(OAuthDeviceCodeError::TimedOut.to_string(), "Device flow timed out");
    assert_eq!(
        OAuthDeviceCodeError::SlowDownTimedOut.to_string(),
        "Device flow timed out after one or more slow_down responses. This is often caused by clock drift in WSL or VM environments. Please sync or restart the VM clock and try again."
    );
    assert_eq!(
        OAuthDeviceCodeError::Failed(("boom".to_owned()).into()).to_string(),
        "boom"
    );
}
