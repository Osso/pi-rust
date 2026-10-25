use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use pi_ai::auth::oauth::device_code::{
    OAuthDeviceCodePollOptions, OAuthDeviceCodePollResult, poll_oauth_device_code_flow,
};
use pi_ai::request::CallbackError;
use pi_js_compat::abort::AbortSignal;

#[tokio::test(start_paused = true)]
async fn device_poll_preserves_a_rejected_callback() {
    use pi_ai::request::CallbackError;
    let error: CallbackError = pi_js_compat::node_error::NativeNodeError::new("TypeError", "fetch failed").into();
    let failure = error.clone();
    let result =
        poll_oauth_device_code_flow::<(), _, _>(options(None, None), move || std::future::ready(Err(failure.clone())))
            .await;
    assert_eq!(result.unwrap_err(), error);
}

use crate::shared_oracle as oracle;

#[tokio::test(start_paused = true)]
async fn device_poll_schedules_and_outcomes_match_frozen() {
    use serde_json::{Value, json};
    use std::sync::Mutex;
    let mut oracle = oracle::TsOracle::spawn("device-code-oracle.mts", "packages/ai/src/utils/oauth/device-code.ts");
    let scenarios = [
        json!({"options":{},"steps":[{"status":"pending"}]}),
        json!({"options":{"waitBeforeFirstPoll":true,"intervalSeconds":2.25},"steps":[]}),
        json!({"options":{"intervalSeconds":0.5},"steps":[{"status":"pending"}]}),
        json!({"options":{"intervalSeconds":2},"steps":[{"status":"slow_down"}]}),
        json!({"options":{"intervalSeconds":2},"steps":[{"status":"slow_down","intervalSeconds":3}]}),
        json!({"options":{"expiresInSeconds":2},"steps":[{"status":"slow_down"}]}),
        json!({"options":{"expiresInSeconds":0},"steps":[]}),
        json!({"options":{"expiresInSeconds":-2},"steps":[]}),
        json!({"options":{},"steps":[{"status":"failed","message":"access denied"}]}),
        json!({"options":{},"steps":[],"abort":true}),
        json!({"options":{},"steps":[],"reject":true}),
    ];
    for request in scenarios {
        let expected = oracle.request(&request);
        let start = tokio::time::Instant::now();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let mut steps = VecDeque::from(request["steps"].as_array().unwrap().clone());
        let mut opts = options(
            request["options"]["intervalSeconds"].as_f64(),
            request["options"]["expiresInSeconds"].as_f64(),
        );
        opts.wait_before_first_poll = request["options"]["waitBeforeFirstPoll"].as_bool().unwrap_or(false);
        let controller = pi_js_compat::abort::AbortController::new();
        opts.signal = controller.signal();
        if request["abort"] == true {
            controller.abort(None);
        }
        let rejects = request["reject"] == true;
        let outcome = poll_oauth_device_code_flow(opts, move || {
            recorded.lock().unwrap().push(start.elapsed().as_millis() as u64);
            let result = if rejects {
                Err(pi_js_compat::node_error::NativeNodeError::new("TypeError", "fetch failed").into())
            } else {
                let step = steps
                    .pop_front()
                    .unwrap_or(json!({"status":"complete","value":"token"}));
                Ok(match step["status"].as_str().unwrap() {
                    "pending" => OAuthDeviceCodePollResult::Pending,
                    "slow_down" => OAuthDeviceCodePollResult::SlowDown {
                        interval_seconds: step["intervalSeconds"].as_f64(),
                    },
                    "failed" => OAuthDeviceCodePollResult::Failed {
                        message: step["message"].as_str().unwrap().into(),
                    },
                    "complete" => OAuthDeviceCodePollResult::Complete(step["value"].clone()),
                    other => panic!("unexpected poll status {other}"),
                })
            };
            std::future::ready(result)
        })
        .await;
        let outcome = match outcome {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => json!({"ok":false,"name":error.name,"message":error.message}),
        };
        let actual: Value =
            json!({"polls":*calls.lock().unwrap(),"elapsed":start.elapsed().as_millis() as u64,"outcome":outcome});
        assert_eq!(actual, expected, "{request}");
    }
}

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
) -> impl FnMut() -> std::future::Ready<Result<OAuthDeviceCodePollResult<T>, CallbackError>> {
    let mut results = VecDeque::from(results);
    move || {
        calls.fetch_add(1, Ordering::SeqCst);
        std::future::ready(Ok(results.pop_front().expect("unexpected extra poll")))
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

    assert_eq!(task.await.unwrap(), Err(CallbackError::new("Login cancelled")));
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

    assert_eq!(task.await.unwrap(), Err(CallbackError::new("Device flow timed out")));
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

    assert_eq!(
        task.await.unwrap(),
        Err(CallbackError::new(
            "Device flow timed out after one or more slow_down responses. This is often caused by clock drift in WSL or VM environments. Please sync or restart the VM clock and try again."
        ))
    );
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

    assert_eq!(result, Err(CallbackError::new("access_denied")));
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
