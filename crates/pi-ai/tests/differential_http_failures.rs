use crate::shared_oracle as oracle;

use std::time::Duration;

use pi_ai::http::default_fetch;
use pi_ai::request::{CallbackError, HttpRequest};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const BOUND: Duration = Duration::from_secs(4);

fn request(url: &str) -> HttpRequest {
    HttpRequest {
        method: "GET".to_owned(),
        url: url.to_owned(),
        headers: Vec::new(),
        body: None,
    }
}

async fn source_failure(url: &str) -> Value {
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut source = oracle::TsOracle::spawn("native-http-failure-oracle.ts", "packages/ai/src/types.ts");
        let result = source.request(&json!({"url": url}));
        eprintln!("Node evidence: {result}");
        result
    })
    .await
    .expect("source task")
}

fn assert_failure(error: &CallbackError, source: &Value, events: &[&str], status: Option<u16>) {
    assert_eq!(source["events"], json!(events));
    assert_eq!(source["status"], json!(status));
    assert!(source["error"]["cause"].is_object(), "retain Node cause: {source}");
    assert!(source["ownError"]["stack"].is_string());
    assert!(source["stderr"].as_str().expect("raw stderr").contains("TypeError"));
    eprintln!("Native evidence: {error:?}");
    assert_eq!(
        error.message,
        pi_js_compat::js_json::JsString::from(source["error"]["message"].as_str().unwrap())
    );
    assert_eq!(error.name, source["error"]["name"].as_str().unwrap());
    assert_eq!(
        error
            .thrown_value()
            .as_error()
            .unwrap()
            .is_instance_of(pi_js_compat::node_error::NativeErrorClass::TypeError),
        source["typeError"].as_bool().unwrap()
    );
    assert_eq!(
        error.to_string(),
        pi_js_compat::js_json::encode_node_utf8(&error.message),
        "Display stays message-only"
    );
    let cause = error.cause.as_deref().expect("native cause is retained");
    assert!(!cause.is_empty());
    for secret in [
        "http://",
        "private-secret",
        "query-secret",
        "user-secret",
        "password-secret",
    ] {
        assert!(!cause.contains(secret), "URL leaked through cause: {cause}");
    }
}

#[tokio::test]
async fn refused_connection_matches_native_fetch_failure_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind closed port");
    let url = format!(
        "http://{}/private-secret?token=query-secret",
        listener.local_addr().expect("address")
    );
    drop(listener);
    let source = source_failure(&url).await;
    assert_eq!(source["error"]["name"], "TypeError");
    assert_eq!(source["error"]["message"], "fetch failed");
    assert_eq!(source["error"]["cause"]["code"], "ECONNREFUSED");
    let native_url = url.replace("http://", "http://user-secret:password-secret@");
    let error = tokio::time::timeout(BOUND, default_fetch().fetch(request(&native_url), None))
        .await
        .expect("bounded fetch")
        .err()
        .expect("closed port fails during send");
    assert_failure(&error, &source, &["fetch", "error"], None);
    assert!(
        error
            .cause
            .as_deref()
            .expect("native cause")
            .contains("Connection refused")
    );
}

struct PrematureBody {
    url: String,
    server: tokio::task::JoinHandle<()>,
}

impl PrematureBody {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind premature body");
        let url = format!(
            "http://{}/private-secret?token=query-secret",
            listener.local_addr().expect("address")
        );
        let server = tokio::spawn(async move {
            // One real connection each for Node and Rust, with identical bytes/EOF.
            for _ in 0..2 {
                tokio::time::timeout(BOUND, async {
                    let (mut socket, _) = listener.accept().await.expect("accept");
                    let mut bytes = Vec::new();
                    let mut chunk = [0; 1024];
                    while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                        let read = socket.read(&mut chunk).await.expect("request read");
                        assert_ne!(read, 0, "request headers must arrive");
                        bytes.extend_from_slice(&chunk[..read]);
                    }
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc")
                        .await
                        .expect("response write");
                    // Headers must resolve fetch before the incomplete body's EOF.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    socket.shutdown().await.expect("EOF");
                })
                .await
                .expect("bounded fixture connection");
            }
        });
        Self { url, server }
    }
}

impl Drop for PrematureBody {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[tokio::test]
async fn premature_body_matches_native_fetch_failure_identity_after_headers() {
    let mut fixture = PrematureBody::start().await;
    let source = source_failure(&fixture.url).await;
    assert_eq!(source["error"]["name"], "TypeError");
    assert_eq!(source["error"]["message"], "terminated");
    let (status, bytes, error) = tokio::time::timeout(BOUND, async {
        let mut response = default_fetch()
            .fetch(request(&fixture.url), None)
            .await
            .expect("headers resolve");
        let mut bytes = Vec::new();
        loop {
            match response.body.next_chunk().await.expect("premature EOF must fail") {
                Ok(chunk) => bytes.extend(chunk),
                Err(error) => break (response.status, bytes, error),
            }
        }
    })
    .await
    .expect("bounded body read");
    assert_eq!(status, 200);
    assert_eq!(bytes, b"abc");
    assert_failure(&error, &source, &["fetch", "response", "body", "error"], Some(status));
    assert!(
        error
            .cause
            .as_deref()
            .expect("native cause")
            .contains("end of file before message length reached")
    );
    (&mut fixture.server).await.expect("fixture finished");
}
