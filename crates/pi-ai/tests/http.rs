use std::time::Duration;

use pi_ai::http::default_fetch;
use pi_ai::request::HttpRequest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

async fn serve_once(response: &'static [u8]) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = vec![0_u8; 4096];
        let read = socket.read(&mut request).await.expect("read request");
        request.truncate(read);
        socket.write_all(response).await.expect("write response");
        socket.shutdown().await.expect("shutdown");
        request
    });
    (format!("http://{address}/v1/messages"), handle)
}

#[tokio::test]
async fn default_fetch_sends_method_headers_body_and_streams_response() {
    let (url, server) = serve_once(
        b"HTTP/1.1 201 Created\r\nContent-Type: text/event-stream\r\nX-Request-Id: req_1\r\nTransfer-Encoding: chunked\r\n\r\n6\r\ndata: \r\n3\r\nhi\n\r\n0\r\n\r\n",
    )
    .await;
    let request = HttpRequest {
        method: "POST".to_owned(),
        url,
        headers: vec![("x-api-key".to_owned(), "secret".to_owned())],
        body: Some(br#"{"a":1}"#.to_vec()),
    };
    let mut response = default_fetch().fetch(request, None).await.expect("fetch");
    assert_eq!(response.status, 201);
    assert!(
        response
            .headers
            .contains(&("x-request-id".to_owned(), "req_1".to_owned()))
    );

    let mut body = Vec::new();
    while let Some(chunk) = response.body.next_chunk().await {
        body.extend(chunk.expect("chunk"));
    }
    assert_eq!(body, b"data: hi\n");

    let raw_request = String::from_utf8(server.await.expect("server")).expect("utf8");
    assert!(raw_request.starts_with("POST /v1/messages HTTP/1.1\r\n"));
    assert!(raw_request.contains("x-api-key: secret\r\n"));
    assert!(raw_request.ends_with(r#"{"a":1}"#));
}

#[tokio::test]
async fn cancelled_signal_aborts_pending_fetch() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}/slow", listener.local_addr().expect("address"));
    let server = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.expect("accept");
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
    let signal = CancellationToken::new();
    let cancel = signal.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel.cancel();
    });
    let request = HttpRequest {
        method: "GET".to_owned(),
        url,
        headers: Vec::new(),
        body: None,
    };
    let result = tokio::time::timeout(Duration::from_secs(5), default_fetch().fetch(request, Some(signal)))
        .await
        .expect("fetch must finish promptly after abort");
    server.abort();
    assert!(server.await.expect_err("server cancelled").is_cancelled());
    let error = result.err().expect("aborted fetch must fail");
    assert_eq!(error.message, "This operation was aborted");
    assert_eq!(error.name, "Error");
    assert_eq!(error.cause, None);
}

#[test]
fn callback_error_constructor_and_display_keep_the_legacy_contract() {
    let error = pi_ai::request::CallbackError::new("callback failed");
    assert_eq!(error.message, "callback failed");
    assert_eq!(error.name, "Error");
    assert_eq!(error.cause, None);
    assert_eq!(error.to_string(), "callback failed");
    assert_eq!(error.clone(), error);
}

#[tokio::test]
async fn request_builder_errors_are_not_reclassified_as_network_failures() {
    for (method, url, headers) in [
        ("GET\n", "http://127.0.0.1:1/", Vec::new()),
        ("GET", "not a url", Vec::new()),
        (
            "GET",
            "http://127.0.0.1:1/",
            vec![("bad\nheader".to_owned(), "value".to_owned())],
        ),
    ] {
        let error = default_fetch()
            .fetch(
                HttpRequest {
                    method: method.to_owned(),
                    url: url.to_owned(),
                    headers,
                    body: None,
                },
                None,
            )
            .await
            .err()
            .expect("invalid builder input fails");
        assert_eq!(error.name, "Error");
        assert_eq!(error.cause, None);
        assert_ne!(error.message, "fetch failed");
        assert!(!error.message.is_empty());
    }
}
