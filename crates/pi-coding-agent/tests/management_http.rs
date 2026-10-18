use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use pi_coding_agent::utils::management_http::{
    FetchError, FetchRetryOptions, HttpFetcher, HttpRequest, HttpResponse, UreqFetcher, fetch_with_retry,
};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct MockFetcher {
    results: Mutex<VecDeque<Result<HttpResponse, FetchError>>>,
    timeouts: Mutex<Vec<Option<Duration>>>,
    calls: AtomicUsize,
}

impl MockFetcher {
    fn with_results(results: Vec<Result<HttpResponse, FetchError>>) -> Self {
        Self {
            results: Mutex::new(results.into()),
            timeouts: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn timeouts(&self) -> Vec<Option<Duration>> {
        self.timeouts.lock().expect("timeouts lock").clone()
    }
}

impl HttpFetcher for MockFetcher {
    fn fetch(&self, _request: &HttpRequest, timeout: Option<Duration>) -> Result<HttpResponse, FetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.timeouts.lock().expect("timeouts lock").push(timeout);
        self.results
            .lock()
            .expect("results lock")
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected fetch call"))
    }
}

fn ok(status: u16, body: &str) -> Result<HttpResponse, FetchError> {
    Ok(HttpResponse {
        status,
        body: body.as_bytes().to_vec(),
    })
}

fn transport(message: &str) -> Result<HttpResponse, FetchError> {
    Err(FetchError::Transport(message.to_owned()))
}

#[test]
fn retries_a_transient_transport_failure_twice_by_default() {
    let fetcher = MockFetcher::with_results(vec![
        transport("fetch failed"),
        transport("fetch failed"),
        ok(200, "ok"),
    ]);

    let response = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions::default(),
    )
    .expect("eventual response");

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
    assert_eq!(fetcher.calls(), 3);
}

#[test]
fn shares_the_timeout_budget_across_attempts() {
    let fetcher = MockFetcher::with_results(vec![transport("fetch failed"), ok(200, "ok")]);

    let response = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions {
            timeout_ms: Some(1_000),
            ..FetchRetryOptions::default()
        },
    )
    .expect("eventual response");

    assert_eq!(response.status, 200);
    assert_eq!(fetcher.calls(), 2);
    let timeouts = fetcher.timeouts();
    let first = timeouts[0].expect("first timeout");
    let second = timeouts[1].expect("second timeout");
    assert!(first <= Duration::from_millis(1_000));
    assert!(second > Duration::ZERO);
    assert!(second <= first);
}

#[test]
fn retries_transient_http_statuses_and_returns_the_successful_response() {
    let fetcher = MockFetcher::with_results(vec![ok(503, "busy"), ok(200, "ok")]);

    let response = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions::default(),
    )
    .expect("eventual response");

    assert_eq!(response.status, 200);
    assert_eq!(fetcher.calls(), 2);
}

#[test]
fn returns_the_last_transient_status_after_exhausting_retries() {
    let fetcher = MockFetcher::with_results(vec![ok(503, "busy"), ok(503, "busy")]);

    let response = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions {
            max_retries: Some(1),
            ..FetchRetryOptions::default()
        },
    )
    .expect("final response");

    assert_eq!(response.status, 503);
    assert_eq!(fetcher.calls(), 2);
}

#[test]
fn does_not_retry_non_transient_statuses() {
    let fetcher = MockFetcher::with_results(vec![ok(404, "missing")]);

    let response = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions::default(),
    )
    .expect("response");

    assert_eq!(response.status, 404);
    assert_eq!(fetcher.calls(), 1);
}

#[test]
fn can_disable_status_retries() {
    let fetcher = MockFetcher::with_results(vec![ok(503, "busy")]);

    let response = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions {
            retry_on_status: false,
            ..FetchRetryOptions::default()
        },
    )
    .expect("response");

    assert_eq!(response.status, 503);
    assert_eq!(fetcher.calls(), 1);
}

#[test]
fn caller_cancellation_is_terminal_and_never_calls_the_fetcher() {
    let token = CancellationToken::new();
    token.cancel();
    let fetcher = MockFetcher::with_results(vec![ok(200, "unused")]);

    let error = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions {
            cancellation: Some(token),
            ..FetchRetryOptions::default()
        },
    )
    .expect_err("cancelled request");

    assert_eq!(error, FetchError::Cancelled);
    assert_eq!(fetcher.calls(), 0);
}

#[test]
fn transport_failure_exhaustion_returns_the_last_error() {
    let fetcher = MockFetcher::with_results(vec![transport("first"), transport("second")]);

    let error = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions {
            max_retries: Some(1),
            ..FetchRetryOptions::default()
        },
    )
    .expect_err("transport failure");

    assert_eq!(error, FetchError::Transport("second".to_owned()));
    assert_eq!(fetcher.calls(), 2);
}

#[test]
fn zero_or_absent_timeout_imposes_no_deadline() {
    let fetcher = MockFetcher::with_results(vec![ok(200, "ok")]);
    let response = fetch_with_retry(
        &fetcher,
        &HttpRequest::new("https://example.test"),
        &FetchRetryOptions {
            timeout_ms: Some(0),
            ..FetchRetryOptions::default()
        },
    )
    .expect("response");
    assert_eq!(response.status, 200);
    assert_eq!(fetcher.timeouts(), vec![None]);
}

#[test]
fn ureq_fetcher_retries_a_real_transient_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let requests = std::thread::spawn(move || {
        for attempt in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let body = if attempt == 0 { "busy" } else { "ok" };
            let status = if attempt == 0 { 503 } else { 200 };
            write!(
                stream,
                "HTTP/1.1 {status} TEST\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write response");
        }
    });

    let fetcher = UreqFetcher::new();
    let response = fetch_with_retry(
        &fetcher,
        &HttpRequest::new(format!("http://{address}/resource")),
        &FetchRetryOptions {
            timeout_ms: Some(5_000),
            ..FetchRetryOptions::default()
        },
    )
    .expect("eventual response");

    requests.join().expect("server thread");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
}
