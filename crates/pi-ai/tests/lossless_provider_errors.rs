use crate::shared_oracle as oracle;
#[path = "support/provider_normalize.rs"]
pub mod provider_normalize;
use pi_ai::types::{AssistantMessageEvent, Context, Model, StopReason};
use pi_js_compat::js_json::{JsString, parse_lossless};
use std::io::{Read, Write};
fn serve_sse(events: &str) -> (String, std::thread::JoinHandle<()>) {
    serve_gated_sse(events, None)
}

fn serve_gated_sse(events: &str, gate: Option<std::sync::mpsc::Receiver<()>>) -> (String, std::thread::JoinHandle<()>) {
    serve_http(events, gate, 200)
}

fn serve_http(
    events: &str,
    gate: Option<std::sync::mpsc::Receiver<()>>,
    status: u16,
) -> (String, std::thread::JoinHandle<()>) {
    serve_http_with_disconnect(events, gate, status, false)
}

fn serve_http_with_disconnect(
    events: &str,
    gate: Option<std::sync::mpsc::Receiver<()>>,
    status: u16,
    disconnect_expected: bool,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let events = events.to_owned();
    let task = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let headers = String::from_utf8(request).unwrap();
        let length: usize = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|n| n.trim().parse().unwrap())
            })
            .unwrap();
        socket.read_exact(&mut vec![0; length]).unwrap();
        socket.write_all(format!("HTTP/1.1 {status} Probe\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
        for byte in events.bytes() {
            if let Err(error) = socket.write_all(&[b'1', b'\r', b'\n', byte, b'\r', b'\n']) {
                if disconnect_expected
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                    )
                {
                    return;
                }
                panic!("unexpected SSE write failure: {error}");
            }
        }
        if let Some(gate) = gate {
            gate.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        }
        if let Err(error) = socket.write_all(b"0\r\n\r\n") {
            assert!(
                disconnect_expected
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                    ),
                "unexpected SSE end failure: {error}"
            );
        }
    });
    (format!("http://{address}/v1"), task)
}

fn test_model(api: &str, url: &str) -> Model {
    serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":api,"provider":"openai","baseUrl":url,"reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap()
}

#[tokio::test]
async fn sse_human_error_is_persistent_and_final_exact_utf16() {
    for api in [
        "openai-completions",
        "openai-responses",
        "azure-openai-responses",
        "openai-codex-responses",
    ] {
        let event = if api == "openai-codex-responses" {
            r#"{"type":"error","code":"bad_request","message":"human\ud800end\udfff"}"#
        } else {
            r#"{"error":{"message":"human\ud800end\udfff","type":"bad_request"}}"#
        };
        let (url, server) = serve_sse(&format!("data: {event}\n\n"));
        let mut options = pi_ai::request::StreamOptions::default();
        options.request.api_key = Some("fake".into());
        options.request.max_retries = Some(0.0);
        if api == "openai-codex-responses" {
            let claims =
                pi_js_compat::base64::btoa(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"fake-account"}}"#);
            options.request.api_key = Some((format!("fake.{claims}.fake")).into());
            options.transport = Some(pi_ai::types::Transport::Sse);
        }
        let context = Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        };
        let model = test_model(api, &url);
        let mut stream = match api {
            "azure-openai-responses" => pi_ai::api::azure_openai_responses::stream(model, context, options),
            "openai-responses" => pi_ai::api::openai_responses::stream(model, context, options),
            "openai-codex-responses" => pi_ai::api::openai_codex_responses::stream(
                model,
                context,
                pi_ai::api::openai_codex_responses::OpenAICodexResponsesOptions {
                    stream: options,
                    ..Default::default()
                },
            ),
            _ => pi_ai::api::openai_completions::stream(model, context, options),
        };
        let mut errors = vec![];
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while let Some(event) = stream.next().await {
                if let AssistantMessageEvent::Error { error, .. } = event.unwrap() {
                    errors.push(error);
                }
            }
        })
        .await
        .unwrap();
        server.join().unwrap();
        let result = stream.result().await.unwrap();
        assert_eq!(result.stop_reason, StopReason::Error, "{api}");
        assert_eq!(errors.len(), 1, "{api}");
        for message in [&result, &errors[0]] {
            let wire = parse_lossless(serde_json::to_string(message).unwrap()).unwrap();
            assert_eq!(
                wire.get("errorMessage").unwrap().as_string().unwrap().units(),
                JsString::join(
                    [
                        if api == "openai-codex-responses" {
                            "Codex error: "
                        } else {
                            ""
                        }
                        .into(),
                        JsString::from_utf16(vec![104, 117, 109, 97, 110, 0xd800, 101, 110, 100, 0xdfff])
                    ],
                    ""
                )
                .units(),
                "{api}"
            );
        }
    }
}

fn options(api: &str) -> pi_ai::request::StreamOptions {
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.max_retries = Some(0.0);
    options.request.api_key = Some(if api == "openai-codex-responses" {
        let claims =
            pi_js_compat::base64::btoa(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"fake-account"}}"#);
        (format!("fake.{claims}.fake")).into()
    } else {
        "fake".into()
    });
    options.transport = Some(pi_ai::types::Transport::Sse);
    options
}

fn stream_for(
    api: &str,
    url: &str,
    options: pi_ai::request::StreamOptions,
) -> pi_ai::utils::event_stream::AssistantMessageEventStream {
    let model = test_model(api, url);
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: None,
    };
    match api {
        "mistral-conversations" => pi_ai::api::mistral_conversations::stream(
            model,
            context,
            pi_ai::api::mistral_conversations::MistralOptions {
                stream: options,
                ..Default::default()
            },
        ),
        "openai-responses" => pi_ai::api::openai_responses::stream(model, context, options),
        "azure-openai-responses" => pi_ai::api::azure_openai_responses::stream(model, context, options),
        "openai-codex-responses" => pi_ai::api::openai_codex_responses::stream(
            model,
            context,
            pi_ai::api::openai_codex_responses::OpenAICodexResponsesOptions {
                stream: options,
                ..Default::default()
            },
        ),
        _ => pi_ai::api::openai_completions::stream(model, context, options),
    }
}

async fn assert_final_error(
    mut stream: pi_ai::utils::event_stream::AssistantMessageEventStream,
    expected: &JsString,
) -> pi_ai::types::AssistantMessage {
    let mut errors = vec![];
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(event) = stream.next().await {
            if let AssistantMessageEvent::Error { error, .. } = event.unwrap() {
                errors.push(error);
            }
        }
    })
    .await
    .unwrap();
    let result = stream.result().await.unwrap();
    assert_eq!(result.stop_reason, StopReason::Error, "API {}", result.api);
    assert_eq!(errors.len(), 1, "API {}", result.api);
    assert_eq!(result.error_message.as_ref(), Some(expected), "API {}", result.api);
    assert_eq!(errors[0].error_message.as_ref(), Some(expected), "API {}", result.api);
    // JSON persistence must carry the exact units, not a Display projection.
    let persisted = parse_lossless(serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(persisted.get("errorMessage").unwrap().as_string(), Some(expected));
    result
}

#[tokio::test]
async fn codex_http_error_message_keeps_unmatched_units() {
    let (url, server) = serve_http(
        r#"{"error":{"message":"human\ud800end\udfff","code":"bad_request"}}"#,
        None,
        400,
    );
    assert_final_error(
        stream_for("openai-codex-responses", &url, options("openai-codex-responses")),
        &JsString::from_utf16(vec![104, 117, 109, 97, 110, 0xd800, 101, 110, 100, 0xdfff]),
    )
    .await;
    server.join().unwrap();
}

#[tokio::test]
async fn sse_parser_failure_keeps_split_surrogate_excerpt() {
    let source = "x12345678😀abcdefghijklmnop";
    let expected = parse_lossless(source).unwrap_err().into_message();
    assert!(expected.units().contains(&0xd800) || expected.units().contains(&0xd83d));
    for api in [
        "openai-completions",
        "openai-responses",
        "openai-codex-responses",
        "mistral-conversations",
    ] {
        let body = format!("data: {source}\n\n");
        let (url, server) = serve_sse(&body);
        let expected = if api == "mistral-conversations" {
            // Frozen mistral-conversations.ts:78–80 consumes the SDK stream;
            // :98 formats its schema error, not a bare JSON.parse failure.
            let mut oracle = oracle::TsOracle::spawn(
                "lossless-provider-errors-oracle.ts",
                "packages/ai/src/api/mistral-conversations.ts",
            );
            let capture = oracle.request(&serde_json::json!({"model":test_model(api, &url),"body":body,"apiKey":options(api).request.api_key.unwrap(),"units":[]}));
            let wire = capture["wire"].as_str().unwrap();
            let path = std::env::temp_dir().join(format!("mistral-parser-source-{}.json", std::process::id()));
            std::fs::write(&path, wire).unwrap();
            eprintln!("frozen Mistral parser capture: {}", path.display());
            parse_lossless(wire).unwrap()["result"]["errorMessage"]
                .as_string()
                .unwrap()
                .clone()
        } else if api == "openai-codex-responses" {
            JsString::join(["Invalid Codex SSE JSON: ".into(), expected.clone()], "")
        } else {
            expected.clone()
        };
        assert_final_error(stream_for(api, &url, options(api)), &expected).await;
        server.join().unwrap();
    }
}

#[tokio::test]
async fn codex_websocket_error_and_parser_failure_keep_native_units() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    for (event, expected) in [
        (
            r#"{"type":"response.failed","response":{"error":{"code":"bad_request","message":"human\ud800end\udfff"}}}"#,
            JsString::from_utf16(vec![104, 117, 109, 97, 110, 0xd800, 101, 110, 100, 0xdfff]),
        ),
        (
            "x12345678😀abcdefghijklmnop",
            JsString::join(
                [
                    "Invalid Codex WebSocket JSON: ".into(),
                    parse_lossless("x12345678😀abcdefghijklmnop")
                        .unwrap_err()
                        .into_message(),
                ],
                "",
            ),
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            assert!(matches!(socket.next().await.unwrap().unwrap(), Message::Text(_)));
            socket.send(Message::Text(event.to_owned().into())).await.unwrap();
            while let Some(message) = socket.next().await {
                if matches!(message, Ok(Message::Close(_)) | Err(_)) {
                    break;
                }
            }
        });
        let mut options = options("openai-codex-responses");
        options.transport = Some(pi_ai::types::Transport::Websocket);
        let _result = assert_final_error(
            stream_for("openai-codex-responses", &format!("http://{address}/v1"), options),
            &expected,
        )
        .await;
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn mistral_http_body_truncation_splits_exact_utf16_unit() {
    let body = format!("{}😀tail", "a".repeat(3999));
    let (url, server) = serve_http(&body, None, 400);
    let expected = JsString::join(
        [
            "Mistral API error (400): ".into(),
            JsString::from(&body).slice(0, 4000),
            "... [truncated 5 chars]".into(),
        ],
        "",
    );
    assert_final_error(
        stream_for("mistral-conversations", &url, options("mistral-conversations")),
        &expected,
    )
    .await;
    server.join().unwrap();
}

fn human_error() -> JsString {
    JsString::from_utf16(vec![104, 117, 109, 97, 110, 0xd800, 101, 110, 100, 0xdfff])
}

#[tokio::test]
async fn callbacks_and_sdk_http_errors_match_frozen_provider_errors() {
    use std::sync::Arc;
    for api in [
        "openai-completions",
        "openai-responses",
        "azure-openai-responses",
        "openai-codex-responses",
        "mistral-conversations",
    ] {
        for callback in ["payload", "response", "http"] {
            let body = if callback == "http" {
                r#"{"error":{"message":"human\ud800end\udfff","code":"bad_request"}}"#
            } else {
                "data: [DONE]\n\n"
            };
            let (url, server) = if callback == "payload" {
                ("http://127.0.0.1:1/v1".into(), None)
            } else {
                let (url, server) = serve_http_with_disconnect(
                    body,
                    None,
                    if callback == "http" { 400 } else { 200 },
                    callback == "response",
                );
                (url, Some(server))
            };
            let mut options = options(api);
            let key = options.request.api_key.clone().unwrap();
            if callback == "payload" {
                options.request.on_payload = Some(Arc::new(|_, _| {
                    Box::pin(async { Err(pi_ai::request::CallbackError::new(human_error())) })
                }));
            }
            if callback == "response" {
                options.request.on_response = Some(Arc::new(|_, _| {
                    Box::pin(async { Err(pi_ai::request::CallbackError::new(human_error())) })
                }));
            }
            let mut source = oracle::TsOracle::spawn(
                "lossless-provider-errors-oracle.ts",
                &format!("packages/ai/src/api/{api}.ts"),
            );
            let reference = source.request(&serde_json::json!({"model":test_model(api, &url),"body":body,"status":if callback == "http" { Some(400) } else {None},"apiKey":key,"callback":callback,"units":human_error().units()}));
            let reference = parse_lossless(reference["wire"].as_str().unwrap()).unwrap();
            let source_message = reference.get("result").unwrap().get("errorMessage");
            if api == "mistral-conversations" && callback == "response" {
                // Frozen mistral-conversations.ts:78–90 has no onResponse invocation;
                // preserve its complete successful capture instead of the retired native callback error.
                assert!(source_message.is_none());
                let path = std::env::temp_dir().join(format!("mistral-onresponse-source-{}.json", std::process::id()));
                std::fs::write(&path, pi_js_compat::js_json::stringify_lossless(&reference)).unwrap();
                eprintln!("frozen Mistral onResponse capture: {}", path.display());
                let mut stream = stream_for(api, &url, options);
                let mut events = Vec::new();
                while let Some(event) = stream.next().await {
                    events.push(serde_json::to_value(event.unwrap()).unwrap());
                }
                let result = stream.result().await.unwrap();
                assert_eq!(result.stop_reason, StopReason::Stop);
                assert!(result.error_message.is_none());
                let mut actual = serde_json::json!({"events":events,"result":result});
                let mut expected = reference.to_serde().unwrap();
                provider_normalize::normalize_capture_clocks(&mut actual);
                provider_normalize::normalize_capture_clocks(&mut expected);
                assert_eq!(actual, expected, "Mistral must ignore onResponse exactly like frozen");
            } else {
                let expected = source_message
                    .unwrap_or_else(|| panic!("missing source error: {api}/{callback}: {reference:?}"))
                    .as_string()
                    .unwrap()
                    .clone();
                assert_final_error(stream_for(api, &url, options), &expected).await;
            }
            if let Some(server) = server {
                server.join().unwrap();
            }
        }
    }
}

#[test]
fn retry_overflow_and_diagnostics_use_native_error_authority() {
    let mut message = pi_ai::types::AssistantMessage::empty("openai-completions", "openai", "probe", 0.0);
    message.stop_reason = StopReason::Error;
    message.error_message = Some(JsString::join(
        ["rate".into(), JsString::from_utf16(vec![0xd800]), "limit".into()],
        "",
    ));
    assert!(pi_ai::utils::retry::is_retryable_assistant_error(&message));
    message.error_message = Some("rate😀limit".into());
    assert!(!pi_ai::utils::retry::is_retryable_assistant_error(&message));
    message.error_message = Some(JsString::join(
        [human_error(), " input token count exceeds the maximum".into()],
        "",
    ));
    assert!(pi_ai::utils::overflow::is_context_overflow(&message, None));
    let thrown = pi_ai::utils::diagnostics::ThrownValue::Error(pi_ai::utils::diagnostics::DiagnosticErrorInfo {
        name: Some("Error".into()),
        message: human_error(),
        stack: None,
        code: None,
    });
    assert_eq!(pi_ai::utils::diagnostics::format_thrown_value(&thrown), human_error());
    let diagnostic = pi_ai::utils::diagnostics::create_assistant_message_diagnostic("provider_error", &thrown, None);
    let persisted = parse_lossless(serde_json::to_string(&diagnostic).unwrap()).unwrap();
    assert_eq!(
        persisted.get("error").unwrap().get("message").unwrap().as_string(),
        Some(&human_error())
    );
}

struct FailFirstFetch(std::sync::atomic::AtomicBool);
impl pi_ai::request::Fetch for FailFirstFetch {
    fn fetch(
        &self,
        request: pi_ai::request::HttpRequest,
        signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
        Box::pin(async move {
            if self.0.swap(false, std::sync::atomic::Ordering::SeqCst) {
                return Err(pi_ai::request::CallbackError::new(human_error()));
            }
            pi_ai::http::default_fetch().fetch(request, signal).await
        })
    }
}

#[tokio::test]
async fn codex_retry_callback_keeps_raw_fetch_failure_message() {
    use std::sync::{Arc, Mutex};
    let (url, server) = serve_http(r#"{"error":{"message":"final","code":"bad_request"}}"#, None, 400);
    let mut options = options("openai-codex-responses");
    options.request.max_retries = Some(1.0);
    options.request.fetch = Some(Arc::new(FailFirstFetch(std::sync::atomic::AtomicBool::new(true))));
    let reasons = Arc::new(Mutex::new(vec![]));
    let observed = reasons.clone();
    options.on_retry = Some(Arc::new(move |event, _| {
        observed.lock().unwrap().push(serde_json::to_string(&event).unwrap());
        Ok(())
    }));
    assert_final_error(stream_for("openai-codex-responses", &url, options), &"final".into()).await;
    server.join().unwrap();
    let reasons = reasons.lock().unwrap();
    assert_eq!(reasons.len(), 1);
    let event = parse_lossless(reasons[0].as_str()).unwrap();
    assert_eq!(event.get("reason").unwrap().as_string(), Some(&human_error()));
}

#[test]
fn provider_inner_payload_and_retry_limit_text_remain_native() {
    use pi_ai::api::openai_sdk::{format_request_error, provider_error_from_body};
    use pi_ai::utils::provider_retry::{ProviderRequestError, RetryDelayLimitError};
    let body = r#"{"error":{"message":"human\ud800end\udfff","metadata":{"raw":"nested\udfff"}}}"#;
    let provider = provider_error_from_body(400, body);
    let expected = parse_lossless(body).unwrap();
    assert_eq!(provider.inner_error.as_deref(), expected.get("error"));
    assert_eq!(provider.message, JsString::join(["400 ".into(), human_error()], ""));
    let limit = ProviderRequestError::<pi_ai::request::CallbackError>::RetryDelayLimit(RetryDelayLimitError {
        delay_ms: 2000.0,
        max_ms: 1000.0,
        provider_message: human_error(),
    });
    assert_eq!(
        format_request_error(&limit, None),
        JsString::join(
            ["Server requested 2s retry delay (max: 1s). ".into(), human_error()],
            ""
        )
    );
}

#[tokio::test]
async fn completions_error_metadata_raw_is_human_text_not_scalar_metadata() {
    let events = r#"data: {"error":{"message":"human\ud800end\udfff","metadata":{"raw":"nested\udfff"}}}

"#;
    let (url, server) = serve_sse(events);
    let expected = JsString::join(
        [human_error(), "\nnested".into(), JsString::from_utf16(vec![0xdfff])],
        "",
    );
    assert_final_error(
        stream_for("openai-completions", &url, options("openai-completions")),
        &expected,
    )
    .await;
    server.join().unwrap();
}

#[tokio::test]
async fn responses_state_machine_human_errors_remain_native() {
    for api in ["openai-responses", "azure-openai-responses"] {
        for (event, prefix) in [
            (
                r#"{"type":"error","code":"bad_request","message":"human\ud800end\udfff"}"#,
                "Error Code bad_request: ",
            ),
            (
                r#"{"type":"response.failed","response":{"status":"failed","error":{"code":"bad_request","message":"human\ud800end\udfff"}}}"#,
                "bad_request: ",
            ),
        ] {
            let body = format!("data: {event}\n\n");
            let (url, server) = serve_sse(&body);
            let expected = JsString::join([prefix.into(), human_error()], "");
            let mut source = oracle::TsOracle::spawn(
                "lossless-provider-errors-oracle.ts",
                &format!("packages/ai/src/api/{api}.ts"),
            );
            let reference = source
                .request(&serde_json::json!({"model":test_model(api, &url),"body":body,"apiKey":"fake","units":[]}));
            let reference = parse_lossless(reference["wire"].as_str().unwrap()).unwrap();
            assert_eq!(
                reference
                    .get("result")
                    .unwrap()
                    .get("errorMessage")
                    .unwrap()
                    .as_string(),
                Some(&expected)
            );
            assert_final_error(stream_for(api, &url, options(api)), &expected).await;
            server.join().unwrap();
        }
    }
}
