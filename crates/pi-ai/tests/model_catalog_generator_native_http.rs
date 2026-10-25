use crate::shared_oracle as oracle;
use pi_ai::model_catalog_generator::{Generator, fetch_native};
use serde_json::json;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn assert_native_error(url: &str, message: &str) {
    let source_url = url.to_owned();
    let source = tokio::task::spawn_blocking(move || {
        let mut source = oracle::TsOracle::spawn("native-http-failure-oracle.ts", "packages/ai/src/types.ts");
        source.request(&json!({"url":source_url}))
    })
    .await
    .unwrap();
    let error = tokio::time::timeout(Duration::from_secs(4), fetch_native(url))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.name, source["error"]["name"].as_str().unwrap());
    assert_eq!(
        error.message.to_utf8().unwrap(),
        source["error"]["message"].as_str().unwrap()
    );
    assert_eq!(error.message.to_utf8().unwrap(), message);
    let cause = error.cause.as_ref().expect("missing native diagnostic cause");
    assert!(!cause.is_empty());
    for secret in ["http://", "private-secret", "query-secret"] {
        assert!(!cause.contains(secret), "{cause}");
    }
    let mut fetch = |_: &str| Err(error.clone());
    let mut generator = Generator::new(&mut fetch);
    assert!(
        generator
            .fetch_open_router_models()
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(generator.logs.len(), 2);
    assert_eq!(
        generator.logs[1][1],
        pi_js_compat::js_json!("Failed to fetch OpenRouter models:")
    );
    assert_eq!(generator.logs[1][2], error.thrown_value().value());
}

#[tokio::test]
async fn generator_native_send_and_body_failures_use_shared_identity_and_sanitized_causes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/private-secret?token=query-secret",
        listener.local_addr().unwrap()
    );
    drop(listener);
    assert_native_error(&url, "fetch failed").await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/private-secret?token=query-secret",
        listener.local_addr().unwrap()
    );
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            tokio::time::timeout(Duration::from_secs(4), async {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = vec![];
                let mut chunk = [0; 1024];
                while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert_ne!(count, 0);
                    bytes.extend_from_slice(&chunk[..count]);
                }
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc")
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(100)).await;
                socket.shutdown().await.unwrap();
            })
            .await
            .unwrap();
        }
    });
    assert_native_error(&url, "terminated").await;
    server.await.unwrap();
}

// Frozen fetchOpenRouterModels reads JSON even when response.status is 503.
#[tokio::test]
async fn unsuccessful_status_reads_broken_body_and_catches_terminated_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 2048];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        socket
            .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc")
            .await
            .unwrap();
        socket.shutdown().await.unwrap();
    });
    let result = tokio::time::timeout(Duration::from_secs(4), fetch_native(&url))
        .await
        .unwrap();
    let error = result.as_ref().unwrap_err();
    assert_eq!(error.name, pi_js_compat::js_json::JsString::from("TypeError"));
    assert_eq!(error.message, pi_js_compat::js_json::JsString::from("terminated"));
    let mut fetch = |_: &str| result.clone();
    let mut generator = Generator::new(&mut fetch);
    assert!(
        generator
            .fetch_open_router_models()
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(generator.logs[1][2], error.thrown_value().value());
    server.await.unwrap();
}
