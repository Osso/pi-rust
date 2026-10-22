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
            socket.write_all(&[b'1', b'\r', b'\n', byte, b'\r', b'\n']).unwrap();
        }
        if let Some(gate) = gate {
            gate.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        }
        socket.write_all(b"0\r\n\r\n").unwrap();
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
            options.request.api_key = Some(format!("fake.{claims}.fake"));
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
        format!("fake.{claims}.fake")
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
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(errors.len(), 1);
    assert_eq!(result.error_message.as_ref(), Some(expected));
    assert_eq!(errors[0].error_message.as_ref(), Some(expected));
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
        let (url, server) = serve_sse(&format!("data: {source}\n\n"));
        let expected = if api == "openai-codex-responses" {
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
