use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use pi_ai::utils::abort::AbortError;
use pi_ai::utils::provider_retry::{
    ProviderError, ProviderErrorHeaders, ProviderRequestError, ProviderRetryOptions, RetryDelayLimitError,
    retry_provider_request,
};

fn provider_error(status: Option<f64>, headers: &[(&str, &str)]) -> ProviderError {
    let mut error_headers = ProviderErrorHeaders::new();
    for (name, value) in headers {
        error_headers.insert(*name, *value);
    }
    ProviderError {
        message: format!("Provider error: {status:?}").into(),
        status,
        headers: error_headers,
        body: None,
        inner_error: None,
    }
}

fn request_sequence(
    responses: Vec<Result<&'static str, ProviderRequestError<()>>>,
    calls: Arc<AtomicUsize>,
) -> impl FnMut() -> std::future::Ready<Result<&'static str, ProviderRequestError<()>>> {
    let mut responses = responses.into_iter();
    move || {
        calls.fetch_add(1, Ordering::SeqCst);
        std::future::ready(responses.next().expect("unexpected extra request"))
    }
}

#[tokio::test(start_paused = true)]
async fn retries_retryable_provider_errors_after_the_server_delay() {
    let calls = Arc::new(AtomicUsize::new(0));
    let request = request_sequence(
        vec![
            Err(ProviderRequestError::Provider(provider_error(
                Some(429.0),
                &[("retry-after-ms", "1000")],
            ))),
            Ok("ok"),
        ],
        Arc::clone(&calls),
    );

    let result = tokio::spawn(retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 1,
            ..ProviderRetryOptions::default()
        },
    ));

    tokio::time::advance(Duration::from_millis(999)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(1)).await;
    assert_eq!(result.await.unwrap(), Ok("ok"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn does_not_retry_errors_marked_non_retryable() {
    let calls = Arc::new(AtomicUsize::new(0));
    let error = provider_error(Some(429.0), &[("x-should-retry", "false")]);
    let request = request_sequence(vec![Err(ProviderRequestError::Provider(error))], Arc::clone(&calls));

    let result = retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 2,
            ..ProviderRetryOptions::default()
        },
    )
    .await;

    assert!(matches!(result, Err(ProviderRequestError::Provider(_))));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn retries_errors_marked_retryable_even_for_client_statuses() {
    let calls = Arc::new(AtomicUsize::new(0));
    let request = request_sequence(
        vec![
            Err(ProviderRequestError::Provider(provider_error(
                Some(400.0),
                &[("x-should-retry", "true"), ("retry-after-ms", "10")],
            ))),
            Ok("ok"),
        ],
        Arc::clone(&calls),
    );

    let result = retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 1,
            ..ProviderRetryOptions::default()
        },
    )
    .await;

    assert_eq!(result, Ok("ok"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn rejects_a_provider_requested_delay_above_the_limit() {
    let calls = Arc::new(AtomicUsize::new(0));
    let request = request_sequence(
        vec![Err(ProviderRequestError::Provider(provider_error(
            Some(429.0),
            &[("retry-after", "277403")],
        )))],
        Arc::clone(&calls),
    );

    let result = retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 1,
            max_retry_delay_ms: Some(1000.0),
            ..ProviderRetryOptions::default()
        },
    )
    .await;

    match result {
        Err(ProviderRequestError::RetryDelayLimit(limit)) => {
            assert_eq!(limit.delay_ms, 277_403_000.0);
            assert_eq!(limit.max_ms, 1000.0);
            assert_eq!(
                limit.to_string(),
                "Server requested 277403s retry delay (max: 1s). Provider error: Some(429.0)"
            );
        }
        other => panic!("expected retry delay limit error, got {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn allows_disabling_the_delay_cap() {
    let calls = Arc::new(AtomicUsize::new(0));
    let request = request_sequence(
        vec![
            Err(ProviderRequestError::Provider(provider_error(
                Some(429.0),
                &[("retry-after", "2")],
            ))),
            Ok("ok"),
        ],
        Arc::clone(&calls),
    );

    let result = tokio::spawn(retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 1,
            max_retry_delay_ms: Some(0.0),
            ..ProviderRetryOptions::default()
        },
    ));

    tokio::time::advance(Duration::from_millis(1999)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(1)).await;
    assert_eq!(result.await.unwrap(), Ok("ok"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn aborts_a_provider_requested_retry_delay() {
    let calls = Arc::new(AtomicUsize::new(0));
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    let request = request_sequence(
        vec![Err(ProviderRequestError::Provider(provider_error(
            Some(429.0),
            &[("retry-after", "277403")],
        )))],
        Arc::clone(&calls),
    );

    let task = tokio::spawn(retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 2,
            max_retry_delay_ms: Some(0.0),
            signal: Some(signal.clone()),
        },
    ));
    tokio::time::advance(Duration::from_millis(0)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    signal_controller.abort(None);
    assert_eq!(task.await.unwrap(), Err(ProviderRequestError::Abort(AbortError::new())));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn does_not_retry_non_provider_errors() {
    let calls = Arc::new(AtomicUsize::new(0));
    let request = request_sequence(vec![Err(ProviderRequestError::Other(()))], Arc::clone(&calls));

    let result = retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 3,
            ..ProviderRetryOptions::default()
        },
    )
    .await;

    assert_eq!(result, Err(ProviderRequestError::Other(())));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn honors_http_date_retry_after_headers() {
    let calls = Arc::new(AtomicUsize::new(0));
    let retry_at = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(3));
    let request = request_sequence(
        vec![
            Err(ProviderRequestError::Provider(provider_error(
                Some(503.0),
                &[("retry-after", &retry_at)],
            ))),
            Ok("ok"),
        ],
        Arc::clone(&calls),
    );

    let task = tokio::spawn(retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 1,
            ..ProviderRetryOptions::default()
        },
    ));
    tokio::time::advance(Duration::from_millis(1000)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_secs(3)).await;

    assert_eq!(task.await.unwrap(), Ok("ok"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn retries_statuses_without_an_explicit_retry_header() {
    for status in [None, Some(408.0), Some(409.0), Some(429.0), Some(500.0), Some(599.0)] {
        let calls = Arc::new(AtomicUsize::new(0));
        let request = request_sequence(
            vec![
                Err(ProviderRequestError::Provider(provider_error(
                    status,
                    &[("retry-after-ms", "1")],
                ))),
                Ok("ok"),
            ],
            Arc::clone(&calls),
        );
        let result = retry_provider_request(
            request,
            ProviderRetryOptions {
                max_retries: 1,
                ..ProviderRetryOptions::default()
            },
        )
        .await;
        assert_eq!(result, Ok("ok"), "status {status:?} must be retryable");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    for status in [Some(400.0), Some(401.0), Some(404.0), Some(499.0)] {
        let calls = Arc::new(AtomicUsize::new(0));
        let request = request_sequence(
            vec![Err(ProviderRequestError::Provider(provider_error(
                status,
                &[("retry-after-ms", "1")],
            )))],
            Arc::clone(&calls),
        );
        let result = retry_provider_request(
            request,
            ProviderRetryOptions {
                max_retries: 1,
                ..ProviderRetryOptions::default()
            },
        )
        .await;
        assert!(
            matches!(result, Err(ProviderRequestError::Provider(_))),
            "status {status:?} must not retry"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn defaults_to_zero_retries() {
    let calls = Arc::new(AtomicUsize::new(0));
    let request = request_sequence(
        vec![Err(ProviderRequestError::Provider(provider_error(Some(429.0), &[])))],
        Arc::clone(&calls),
    );

    let result = retry_provider_request(request, ProviderRetryOptions::default()).await;

    assert!(matches!(result, Err(ProviderRequestError::Provider(_))));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn retry_delay_limit_display_uses_ceiling_seconds() {
    let limit = RetryDelayLimitError {
        delay_ms: 1501.0,
        max_ms: 1000.0,
        provider_message: "boom".to_owned().into(),
    };
    assert_eq!(limit.to_string(), "Server requested 2s retry delay (max: 1s). boom");
}

#[test]
fn headers_are_case_insensitive() {
    let mut headers = ProviderErrorHeaders::new();
    headers.insert("Retry-After-MS", "1500");
    assert_eq!(headers.get("retry-after-ms"), Some("1500"));
    assert_eq!(headers.get("RETRY-AFTER-MS"), Some("1500"));
    assert_eq!(headers.get("missing"), None);
}

fn assert_future_send<F: Future>(future: F) -> F {
    future
}

#[tokio::test]
async fn retry_future_is_send() {
    let calls = Arc::new(AtomicUsize::new(0));
    let request = request_sequence(vec![Ok("ok")], Arc::clone(&calls));
    let future = assert_future_send(retry_provider_request(request, ProviderRetryOptions::default()));
    assert_eq!(future.await, Ok("ok"));
}

#[tokio::test(start_paused = true)]
async fn iso_8601_prefixes_parse_as_seconds_like_parse_float() {
    // `Number.parseFloat("2999-01-01T00:00:00Z")` is 2999, so the `Date.parse`
    // branch is only reached for non-numeric prefixes such as HTTP dates.
    let calls = Arc::new(AtomicUsize::new(0));
    let request = request_sequence(
        vec![Err(ProviderRequestError::Provider(provider_error(
            Some(429.0),
            &[("retry-after", "2999-01-01T00:00:00Z")],
        )))],
        Arc::clone(&calls),
    );

    let result = retry_provider_request(
        request,
        ProviderRetryOptions {
            max_retries: 1,
            ..ProviderRetryOptions::default()
        },
    )
    .await;

    match result {
        Err(ProviderRequestError::RetryDelayLimit(limit)) => assert_eq!(limit.delay_ms, 2_999_000.0),
        other => panic!("expected a retry delay limit error, got {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn non_finite_retry_after_values_exceed_the_cap() {
    // `parseFloat("Infinity")` is `Infinity`, which `validateServerRetryDelayMs` rejects.
    for header in ["retry-after-ms", "retry-after"] {
        let calls = Arc::new(AtomicUsize::new(0));
        let request = request_sequence(
            vec![Err(ProviderRequestError::Provider(provider_error(
                Some(429.0),
                &[(header, "Infinity")],
            )))],
            Arc::clone(&calls),
        );

        let result = retry_provider_request(
            request,
            ProviderRetryOptions {
                max_retries: 1,
                ..ProviderRetryOptions::default()
            },
        )
        .await;

        match result {
            Err(ProviderRequestError::RetryDelayLimit(limit)) => {
                assert!(limit.delay_ms.is_infinite(), "{header}: {}", limit.delay_ms);
                assert_eq!(
                    limit.to_string(),
                    "Server requested Infinitys retry delay (max: 60s). Provider error: Some(429.0)"
                );
            }
            other => panic!("{header}: expected a retry delay limit error, got {other:?}"),
        }
    }
}
