use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use pi_ai::http::default_fetch;
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_coding_agent::utils::management_http::{FetchRetryOptions, TIMEOUT_ERROR_MESSAGE, fetch_with_retry};
use pi_js_compat::abort::AbortSignal;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

enum Scripted {
    Status(u16),
    Fail(&'static str),
    /// Never completes unless the signal cancels it.
    Hang,
}

struct EmptyBody;

impl HttpBody for EmptyBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { None })
    }
}

#[derive(Default)]
struct MockFetch {
    script: Mutex<VecDeque<Scripted>>,
    calls: AtomicUsize,
}

impl MockFetch {
    fn with(script: Vec<Scripted>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Fetch for MockFetch {
    fn fetch(
        &self,
        _request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = self.script.lock().unwrap().pop_front().expect("unexpected fetch call");
        Box::pin(async move {
            match next {
                Scripted::Status(status) => Ok(HttpResponse {
                    status,
                    status_text: String::new(),
                    headers: Vec::new(),
                    body: Box::new(EmptyBody),
                }),
                Scripted::Fail(message) => Err(CallbackError::new(message)),
                Scripted::Hang => {
                    signal.expect("hanging fetch needs a signal").wait().await;
                    Err(CallbackError::new("This operation was aborted"))
                }
            }
        })
    }
}

fn get(url: &str) -> HttpRequest {
    HttpRequest {
        method: "GET".to_owned(),
        url: url.to_owned(),
        headers: Vec::new(),
        body: None,
    }
}

async fn status_of(fetch: &MockFetch, signal: Option<AbortSignal>, options: FetchRetryOptions) -> u16 {
    fetch_with_retry(fetch, get("https://example.test"), signal, &options)
        .await
        .expect("response")
        .status
}

#[tokio::test]
async fn retries_a_transient_transport_failure_twice_by_default() {
    let fetch = MockFetch::with(vec![
        Scripted::Fail("fetch failed"),
        Scripted::Fail("fetch failed"),
        Scripted::Status(200),
    ]);
    assert_eq!(status_of(&fetch, None, FetchRetryOptions::default()).await, 200);
    assert_eq!(fetch.calls(), 3);
}

#[tokio::test]
async fn retries_transient_statuses_until_success() {
    let fetch = MockFetch::with(vec![
        Scripted::Status(503),
        Scripted::Status(429),
        Scripted::Status(200),
    ]);
    assert_eq!(status_of(&fetch, None, FetchRetryOptions::default()).await, 200);
    assert_eq!(fetch.calls(), 3);
}

#[tokio::test]
async fn returns_the_last_transient_status_after_exhausting_retries() {
    let fetch = MockFetch::with(vec![Scripted::Status(503), Scripted::Status(503)]);
    let options = FetchRetryOptions {
        max_retries: Some(1),
        ..FetchRetryOptions::default()
    };
    assert_eq!(status_of(&fetch, None, options).await, 503);
    assert_eq!(fetch.calls(), 2);
}

#[tokio::test]
async fn does_not_retry_non_transient_statuses() {
    for status in [200, 404, 501] {
        let fetch = MockFetch::with(vec![Scripted::Status(status)]);
        assert_eq!(status_of(&fetch, None, FetchRetryOptions::default()).await, status);
        assert_eq!(fetch.calls(), 1);
    }
}

#[tokio::test]
async fn can_disable_status_retries() {
    let fetch = MockFetch::with(vec![Scripted::Status(503)]);
    let options = FetchRetryOptions {
        retry_on_status: false,
        ..FetchRetryOptions::default()
    };
    assert_eq!(status_of(&fetch, None, options).await, 503);
    assert_eq!(fetch.calls(), 1);
}

#[tokio::test]
async fn transport_failure_exhaustion_returns_the_last_error() {
    let fetch = MockFetch::with(vec![Scripted::Fail("first"), Scripted::Fail("second")]);
    let options = FetchRetryOptions {
        max_retries: Some(1),
        ..FetchRetryOptions::default()
    };
    let error = fetch_with_retry(&fetch, get("https://example.test"), None, &options)
        .await
        .err()
        .expect("failure");
    assert_eq!(error.message, "second");
    assert_eq!(fetch.calls(), 2);
}

#[tokio::test]
async fn caller_cancellation_before_start_never_calls_fetch() {
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    controller_signal.abort(None);
    let fetch = MockFetch::with(vec![Scripted::Status(200)]);
    let error = fetch_with_retry(
        &fetch,
        get("https://example.test"),
        Some(signal),
        &FetchRetryOptions::default(),
    )
    .await
    .err()
    .expect("aborted");
    assert_eq!(error.message, "This operation was aborted");
    assert_eq!(fetch.calls(), 0);
}

#[tokio::test]
async fn caller_cancellation_in_flight_is_terminal() {
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let controller_cancel = controller_signal.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        controller_cancel.abort(None);
    });
    let fetch = MockFetch::with(vec![Scripted::Hang, Scripted::Status(200)]);
    let error = fetch_with_retry(
        &fetch,
        get("https://example.test"),
        Some(signal),
        &FetchRetryOptions::default(),
    )
    .await
    .err()
    .expect("aborted");
    assert_eq!(error.message, "This operation was aborted");
    assert_eq!(fetch.calls(), 1);
}

#[tokio::test]
async fn timeout_is_one_budget_shared_by_all_attempts_and_terminal() {
    let fetch = MockFetch::with(vec![
        Scripted::Fail("fetch failed"),
        Scripted::Hang,
        Scripted::Status(200),
    ]);
    let options = FetchRetryOptions {
        timeout_ms: Some(30),
        ..FetchRetryOptions::default()
    };
    let started = std::time::Instant::now();
    let error = fetch_with_retry(&fetch, get("https://example.test"), None, &options)
        .await
        .err()
        .expect("timed out");
    assert_eq!(error.message, TIMEOUT_ERROR_MESSAGE);
    assert_eq!(
        fetch.calls(),
        2,
        "the timeout aborts the in-flight attempt without retrying"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn zero_timeout_imposes_no_deadline() {
    let fetch = MockFetch::with(vec![Scripted::Status(200)]);
    let options = FetchRetryOptions {
        timeout_ms: Some(0),
        ..FetchRetryOptions::default()
    };
    assert_eq!(status_of(&fetch, None, options).await, 200);
}

#[tokio::test]
async fn default_fetch_retries_a_real_transient_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let server = tokio::spawn(async move {
        for (status, body) in [(503, "busy"), (200, "ok")] {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer).await;
            let response = format!(
                "HTTP/1.1 {status} TEST\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.expect("write response");
        }
    });

    let options = FetchRetryOptions {
        timeout_ms: Some(5_000),
        ..FetchRetryOptions::default()
    };
    let fetch = default_fetch();
    let mut response = fetch_with_retry(
        fetch.as_ref(),
        get(&format!("http://{address}/resource")),
        None,
        &options,
    )
    .await
    .expect("eventual response");
    server.await.expect("server");
    assert_eq!(response.status, 200);
    let mut body = Vec::new();
    while let Some(chunk) = response.body.next_chunk().await {
        body.extend(chunk.expect("chunk"));
    }
    assert_eq!(body, b"ok");
}
