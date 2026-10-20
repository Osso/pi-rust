#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::{Generator, GeneratorErrorDetails, GeneratorOptions, fetch_native};
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
    let Some(GeneratorErrorDetails::Transport(cause)) = error.details.as_deref() else {
        panic!("missing native cause: {error:?}")
    };
    assert!(!cause.is_empty());
    for secret in ["http://", "private-secret", "query-secret"] {
        assert!(!cause.contains(secret), "{cause}");
    }
    for strict in [false, true] {
        let mut fetch = |_: &str| Err(error.clone());
        let mut generator = Generator::new(
            GeneratorOptions {
                strict,
                ..Default::default()
            },
            &mut fetch,
        );
        let result = generator.fetch_open_router_models();
        if strict {
            let propagated = result.unwrap_err();
            assert_eq!(propagated.console_message(), error.console_message());
        } else {
            assert!(matches!(result.unwrap(),pi_js_compat::js_json::JsValue::Array(values) if values.is_empty()));
        }
        assert_eq!(generator.logs.len(), 2);
        assert_eq!(
            pi_js_compat::js_json::stringify_lossless(&generator.logs[1]),
            json!(["error", "Failed to fetch OpenRouter models:", error.console_message()]).to_string()
        );
    }
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

#[tokio::test]
async fn unsuccessful_status_is_reported_before_a_broken_body_is_read() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
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
                    .write_all(
                        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabc",
                    )
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(100)).await;
                socket.shutdown().await.unwrap();
            })
            .await
            .unwrap();
        }
    });
    let source_url = url.clone();
    let source=tokio::task::spawn_blocking(move||{
        let mut source=oracle::TsOracle::spawn("model-catalog-generator-oracle.ts","packages/ai/scripts/generate-models.ts");
        source.request(&json!({"operation":"fetchOpenRouterModels","argv":["--strict"],"http":{"https://openrouter.ai/api/v1/models":{"nativeUrl":source_url}}}))
    }).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(4), fetch_native(&url))
        .await
        .unwrap();
    let mut fetch = |_: &str| result.clone();
    let mut generator = Generator::new(
        GeneratorOptions {
            strict: true,
            ..Default::default()
        },
        &mut fetch,
    );
    let actual = generator.fetch_open_router_models().unwrap_err();
    assert_eq!(actual.name, source["error"]["name"].as_str().unwrap());
    assert_eq!(
        actual.message.to_utf8().unwrap(),
        source["error"]["message"].as_str().unwrap()
    );
    assert_eq!(
        pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::JsValue::Array(generator.logs)),
        source["logs"].to_string()
    );
    server.await.unwrap();
}
