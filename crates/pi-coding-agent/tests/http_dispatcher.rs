use pi_js_compat::js_json::JsString;
use std::time::Duration;

use pi_ai::request::{Fetch, HttpRequest};
use pi_coding_agent::core::http_dispatcher::{
    DEFAULT_HTTP_IDLE_TIMEOUT_MS, HTTP_IDLE_TIMEOUT_CHOICES, HttpIdleTimeout, configure_http_dispatcher,
    format_http_idle_timeout_ms, parse_http_idle_timeout_ms, resolve_http_proxy_settings,
};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

#[test]
fn exposes_the_source_constants() {
    assert_eq!(DEFAULT_HTTP_IDLE_TIMEOUT_MS, 300_000);
    assert_eq!(
        HTTP_IDLE_TIMEOUT_CHOICES,
        [
            ("30 sec", 30_000),
            ("1 min", 60_000),
            ("2 min", 120_000),
            ("5 min", 300_000),
            ("disabled", 0)
        ]
    );
}

#[test]
fn parses_string_and_numeric_timeouts() {
    let cases: [(&str, Option<u64>); 16] = [
        ("disabled", Some(0)),
        ("DISABLED", Some(0)),
        (" disabled ", Some(0)),
        ("", None),
        ("   ", None),
        ("30000", Some(30_000)),
        (" 30000 ", Some(30_000)),
        ("1.9", Some(1)),
        ("-1", None),
        ("0", Some(0)),
        ("abc", None),
        ("1e3", Some(1000)),
        ("0x10", Some(16)),
        ("Infinity", None),
        ("-Infinity", None),
        ("NaN", None),
    ];
    for (input, expected) in cases {
        assert_eq!(
            parse_http_idle_timeout_ms(HttpIdleTimeout::String(input)),
            expected,
            "{input}"
        );
    }
}

#[test]
fn parses_numeric_timeouts() {
    assert_eq!(parse_http_idle_timeout_ms(HttpIdleTimeout::Number(0.0)), Some(0));
    assert_eq!(parse_http_idle_timeout_ms(HttpIdleTimeout::Number(1500.9)), Some(1500));
    assert_eq!(parse_http_idle_timeout_ms(HttpIdleTimeout::Number(-0.5)), None);
    assert_eq!(parse_http_idle_timeout_ms(HttpIdleTimeout::Number(f64::NAN)), None);
    assert_eq!(parse_http_idle_timeout_ms(HttpIdleTimeout::Number(f64::INFINITY)), None);
}

#[test]
fn formats_known_choices_and_derived_seconds() {
    assert_eq!(format_http_idle_timeout_ms(30_000.0), "30 sec");
    assert_eq!(format_http_idle_timeout_ms(60_000.0), "1 min");
    assert_eq!(format_http_idle_timeout_ms(300_000.0), "5 min");
    assert_eq!(format_http_idle_timeout_ms(0.0), "disabled");
    assert_eq!(format_http_idle_timeout_ms(45_000.0), "45 sec");
    assert_eq!(format_http_idle_timeout_ms(1500.0), "1.5 sec");
    assert_eq!(format_http_idle_timeout_ms(1234.0), "1.234 sec");
}

#[test]
fn applies_proxy_settings_only_to_unset_variables() {
    let applied = resolve_http_proxy_settings(Some("http://127.0.0.1:7890"), None, None);
    assert_eq!(applied.http_proxy.as_deref(), Some("http://127.0.0.1:7890"));
    assert_eq!(applied.https_proxy.as_deref(), Some("http://127.0.0.1:7890"));

    let preserved = resolve_http_proxy_settings(
        Some("http://settings:7890"),
        Some("http://env-http:8080"),
        Some("http://env-https:8080"),
    );
    assert_eq!(preserved.http_proxy.as_deref(), Some("http://env-http:8080"));
    assert_eq!(preserved.https_proxy.as_deref(), Some("http://env-https:8080"));

    let partial = resolve_http_proxy_settings(Some("http://settings:7890"), Some("http://env-http:8080"), None);
    assert_eq!(partial.http_proxy.as_deref(), Some("http://env-http:8080"));
    assert_eq!(partial.https_proxy.as_deref(), Some("http://settings:7890"));

    let blank = resolve_http_proxy_settings(Some("   "), None, None);
    assert_eq!(blank.http_proxy, None);
    assert_eq!(blank.https_proxy, None);
}

#[test]
fn rejects_invalid_dispatcher_timeouts_with_the_source_message() {
    for (value, message) in [
        (-1.0, "Invalid HTTP idle timeout: -1"),
        (f64::NAN, "Invalid HTTP idle timeout: NaN"),
        (f64::INFINITY, "Invalid HTTP idle timeout: Infinity"),
        (-0.5, "Invalid HTTP idle timeout: -0.5"),
    ] {
        match configure_http_dispatcher(value) {
            Ok(_) => panic!("{value} must be rejected"),
            Err(error) => assert_eq!(error, message),
        }
    }
    assert!(configure_http_dispatcher(1.5).is_ok());
    assert!(configure_http_dispatcher(0.0).is_ok());
    let _ = DEFAULT_HTTP_IDLE_TIMEOUT_MS;
}

#[tokio::test]
async fn idle_timeout_aborts_a_stalled_response_body() {
    let (url, server) = start_stalled_server().await;
    let fetch = configure_http_dispatcher(50.0).expect("dispatcher must build");
    let started = std::time::Instant::now();
    let result = body_text(fetch.as_ref(), request(&url)).await;
    assert!(result.is_err(), "a stalled body must time out: {result:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the idle timeout must fire well before the server closes"
    );
    drop(server);
}

#[tokio::test]
async fn disabled_timeout_keeps_waiting_for_the_body() {
    let (url, server) = start_stalled_server().await;
    let fetch = configure_http_dispatcher(0.0).expect("dispatcher must build");
    let outcome = tokio::time::timeout(Duration::from_millis(300), body_text(fetch.as_ref(), request(&url))).await;
    assert!(outcome.is_err(), "without a timeout the read must still be pending");
    drop(server);
}

async fn body_text(fetch: &dyn Fetch, request: HttpRequest) -> Result<String, JsString> {
    let mut response = fetch.fetch(request, None).await.map_err(|error| error.message)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.body.next_chunk().await {
        bytes.extend(chunk.map_err(|error| error.message)?);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn request(url: &str) -> HttpRequest {
    HttpRequest {
        method: "GET".to_owned(),
        url: url.to_owned(),
        headers: Vec::new(),
        body: None,
    }
}

async fn start_stalled_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\npartial")
                .await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
    (format!("http://{address}/"), handle)
}

#[test]
fn trims_and_parses_like_javascript_number() {
    // `String.prototype.trim` strips U+FEFF; `Number("0x")` and `Number("1_000")` are NaN.
    let cases: [(&str, Option<u64>); 4] = [
        ("\u{feff}30000\u{feff}", Some(30_000)),
        ("0x", None),
        ("1_000", None),
        ("0b101", Some(5)),
    ];
    for (input, expected) in cases {
        assert_eq!(
            parse_http_idle_timeout_ms(HttpIdleTimeout::String(input)),
            expected,
            "{input:?}"
        );
    }
}

#[test]
fn invalid_timeout_errors_print_the_number_like_javascript() {
    let error = configure_http_dispatcher(-1e21)
        .err()
        .expect("negative timeout is invalid");
    assert_eq!(error, "Invalid HTTP idle timeout: -1e+21");
}

#[test]
fn configured_default_routes_provider_traffic_through_settings_proxy() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "http_dispatcher::settings_proxy_child", "--nocapture"])
        .env("PI_SETTINGS_PROXY_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap()
    );
}

#[test]
fn settings_proxy_child() {
    if std::env::var("PI_SETTINGS_PROXY_CHILD").as_deref() != Ok("1") {
        return;
    }
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
        let origin_url = format!("http://{}", origin.local_addr().unwrap());
        async fn serve(listener: TcpListener, content: &str) -> Option<String> {
            use tokio::io::AsyncReadExt;
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept()).await.ok()?.ok()?;
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).await.unwrap();
                if count == 0 { break; }
                request.extend_from_slice(&chunk[..count]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") { break; }
            }
            let body = format!("data: {{\"id\":\"fixture\",\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{content}\"}},\"finish_reason\":null}}]}}\n\ndata: {{\"id\":\"fixture\",\"choices\":[{{\"index\":0,\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n");
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
            Some(String::from_utf8(request).unwrap())
        }
        let proxy_task = tokio::spawn(serve(proxy, "through proxy"));
        let origin_task = tokio::spawn(serve(origin, "direct"));
        for name in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy", "NO_PROXY", "no_proxy"] {
            pi_js_compat::process_env::remove(name);
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("settings.json"), serde_json::json!({"httpProxy":proxy_url}).to_string()).unwrap();
        pi_js_compat::process_env::set("PI_CODING_AGENT_DIR", root.path().to_str().unwrap());
        pi_js_compat::process_env::set("PI_CODING_AGENT_STATE_DIR", root.path().join("state").to_str().unwrap());
        assert_eq!(pi_coding_agent::main_runner::main(&["--version".to_owned()], Default::default()).await.unwrap(), 0);
        let model: pi_ai::types::Model = serde_json::from_value(serde_json::json!({
            "id":"loopback", "name":"loopback", "api":"openai-completions", "provider":"fixture",
            "baseUrl":format!("{origin_url}/v1"), "reasoning":false, "input":["text"],
            "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}, "contextWindow":4096,"maxTokens":128
        })).unwrap();
        let context = serde_json::from_value(serde_json::json!({"messages":[{"role":"user","content":"hello","timestamp":1}]})).unwrap();
        let mut options = pi_ai::request::StreamOptions::default();
        options.request.api_key = Some("fixture-key".into());
        options.request.max_retries = Some(0.0);
        let response = tokio::time::timeout(Duration::from_secs(5), pi_ai::api::openai_completions::stream(model, context, options).result()).await.unwrap();
        let seen = proxy_task.await.unwrap();
        origin_task.abort();
        assert!(seen.as_ref().is_some_and(|request| request.starts_with(&format!("POST {origin_url}/v1/chat/completions "))), "proxy saw {seen:?}; response {response:?}");
        let response = response.unwrap();
        assert!(response.content.iter().any(|part| matches!(part, pi_ai::types::AssistantContent::Text(text) if text.text == "through proxy")), "{response:?}");

        // Reconfiguration must affect default_fetch, not only the returned handle.
        pi_js_compat::process_env::remove("HTTP_PROXY");
        pi_js_compat::process_env::remove("HTTPS_PROXY");
        let (url, server) = start_stalled_server().await;
        configure_http_dispatcher(40.0).unwrap();
        let fetch = pi_ai::http::default_fetch();
        let result = tokio::time::timeout(Duration::from_secs(1), body_text(fetch.as_ref(), request(&url))).await;
        server.abort();
        assert!(matches!(result, Ok(Err(_))), "installed default must enforce the new idle timeout: {result:?}");
    });
}
