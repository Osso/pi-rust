use crate::shared_oracle as oracle;

use pi_ai::types::{AssistantContent, AssistantMessageEvent, Context, Model, StopReason};
use pi_js_compat::js_json::parse_lossless;
use std::io::{Read, Write};

fn serve_sse(events: &str) -> (String, std::thread::JoinHandle<()>) {
    serve_gated_sse(events, None)
}

fn serve_gated_sse(events: &str, gate: Option<std::sync::mpsc::Receiver<()>>) -> (String, std::thread::JoinHandle<()>) {
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
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
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

#[tokio::test]
async fn completions_one_byte_sse_preserves_raw_content() {
    assert_chat_ingress("openai-completions").await;
}

#[tokio::test]
async fn mistral_one_byte_sse_sanitizes_text_but_preserves_arguments() {
    assert_chat_ingress("mistral-conversations").await;
}

async fn assert_chat_ingress(api: &str) {
    {
        let events = [
            r#"{"id":"msg","model":"probe","choices":[{"index":0,"finish_reason":null,"delta":{"content":"t\ud800"}}]}"#,
            if api == "openai-completions" { r#"{"id":"msg","model":"probe","choices":[{"index":0,"finish_reason":null,"delta":{"reasoning_content":"h\udfff"}}]}"# } else { r#"{"id":"msg","model":"probe","choices":[{"index":0,"finish_reason":null,"delta":{"content":[{"type":"thinking","thinking":[{"text":"h\udfff"}]}]}}]}"# },
            r#"{"id":"msg","model":"probe","choices":[{"index":0,"finish_reason":null,"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":"probe","arguments":"{\"\ud800\":[{\"nested\":\""}}]}}]}"#,
            r#"{"id":"msg","model":"probe","choices":[{"index":0,"finish_reason":null,"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":"probe","arguments":"\udfff\",\"id\":\"\ud800\",\"name\":\"\udfff\",\"type\":\"\ud800\"}]}"}}]},"finish_reason":"tool_calls"}]}"#,
            "[DONE]",
        ].map(|event| format!("data: {event}\n\n")).concat();
        let (url, server) = serve_sse(&events);
        let model: Model = serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":api,"provider":if api.starts_with("mistral") {"mistral"} else {"openai"},"baseUrl":url,"reasoning":true,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap();
        let mut options = pi_ai::request::StreamOptions::default();
        options.request.api_key = Some("fake-loopback-only".into());
        options.request.max_retries = Some(0.0);
        let context = Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        };
        let oracle_model = serde_json::to_value(&model).unwrap();
        let mut stream = if api == "openai-completions" {
            pi_ai::api::openai_completions::stream(model, context, options)
        } else {
            pi_ai::api::mistral_conversations::stream(
                model,
                context,
                pi_ai::api::mistral_conversations::MistralOptions {
                    stream: options,
                    ..Default::default()
                },
            )
        };
        let mut order = Vec::new();
        let mut emitted_types = Vec::new();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while let Some(event) = stream.next().await {
                let event = event.unwrap();
                let wire = parse_lossless(serde_json::to_string(&event).unwrap()).unwrap();
                emitted_types.push(wire.get("type").unwrap().clone());
                match event {
                    AssistantMessageEvent::TextDelta { delta, .. } => {
                        assert_eq!(
                            delta.units(),
                            if api == "openai-completions" {
                                &[116, 0xd800][..]
                            } else {
                                &[116][..]
                            },
                            "{api}"
                        );
                        order.push("text");
                    }
                    AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                        assert_eq!(
                            delta.units(),
                            if api == "openai-completions" {
                                &[104, 0xdfff][..]
                            } else {
                                &[104][..]
                            },
                            "{api}"
                        );
                        order.push("thinking");
                    }
                    AssistantMessageEvent::ToolCallDelta { .. } => order.push("args"),
                    AssistantMessageEvent::Done { .. } => order.push("done"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        server.join().unwrap();
        let result = stream.result().await.unwrap();
        assert_eq!(
            result.stop_reason,
            StopReason::ToolUse,
            "{api}: {:?}",
            result.error_message
        );
        assert_eq!(order, ["text", "thinking", "args", "args", "done"], "{api}");
        assert!(
            matches!(&result.content[0], AssistantContent::Text(t) if t.text.units() == if api == "openai-completions" { &[116, 0xd800][..] } else { &[116][..] })
        );
        assert!(
            matches!(&result.content[1], AssistantContent::Thinking(t) if t.thinking.units() == if api == "openai-completions" { &[104, 0xdfff][..] } else { &[104][..] })
        );
        let AssistantContent::ToolCall(call) = &result.content[2] else {
            panic!("missing call")
        };
        assert_eq!(
            pi_js_compat::js_json::JsValue::Object(call.arguments.clone()),
            parse_lossless(r#"{"\ud800":[{"nested":"\udfff","id":"\ud800","name":"\udfff","type":"\ud800"}]}"#)
                .unwrap()
        );
        let mut source =
            oracle::TsOracle::spawn("lossless-ingress-oracle.ts", &format!("packages/ai/src/api/{api}.ts"));
        let reference =
            source.request(&serde_json::json!({"model":oracle_model,"sse":events,"apiKey":"fake-loopback-only"}));
        let reference_wire = reference["wire"].as_str().unwrap();
        assert!(!reference_wire.contains("\"stopReason\":\"error\""), "{reference_wire}");
        let reference = parse_lossless(reference_wire).unwrap();
        let actual = parse_lossless(serde_json::to_string(&result).unwrap()).unwrap();
        let source_types: Vec<_> = reference
            .get("events")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event.get("type").unwrap().clone())
            .collect();
        assert_eq!(emitted_types, source_types, "{api}");
        assert_eq!(
            actual.get("content"),
            reference.get("result").unwrap().get("content"),
            "{api}"
        );
    }
}

#[tokio::test]
async fn responses_one_byte_sse_preserves_deltas_and_done_records() {
    let events = response_sse();
    for api in ["openai-responses", "azure-openai-responses", "openai-codex-responses"] {
        let (url, server) = serve_sse(&events);
        let model: Model = serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":api,"provider":if api == "openai-codex-responses" { "openai-codex" } else { "openai" },"baseUrl":url,"reasoning":true,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap();
        let mut options = pi_ai::request::StreamOptions::default();
        options.request.api_key = Some("fake-loopback-only".into());
        options.request.max_retries = Some(0.0);
        if api == "openai-codex-responses" {
            options.request.api_key = Some((fake_codex_token()).into());
            options.transport = Some(pi_ai::types::Transport::Sse);
        }
        let oracle_model = serde_json::to_value(&model).unwrap();
        let api_key = options.request.api_key.clone().unwrap();
        let context = Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        };
        let mut stream = match api {
            "azure-openai-responses" => pi_ai::api::azure_openai_responses::stream(model, context, options),
            "openai-codex-responses" => pi_ai::api::openai_codex_responses::stream(
                model,
                context,
                pi_ai::api::openai_codex_responses::OpenAICodexResponsesOptions {
                    stream: options,
                    ..Default::default()
                },
            ),
            _ => pi_ai::api::openai_responses::stream(model, context, options),
        };
        let mut order = Vec::new();
        let mut emitted_types = Vec::new();
        while let Some(event) = stream.next().await {
            let event = event.unwrap();
            let wire = parse_lossless(serde_json::to_string(&event).unwrap()).unwrap();
            emitted_types.push(wire.get("type").unwrap().clone());
            match event {
                AssistantMessageEvent::TextDelta { delta, .. } => {
                    assert_eq!(delta.units(), &[0xd800]);
                    order.push("text");
                }
                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                    assert_eq!(delta.units(), &[0xdfff]);
                    order.push("thinking");
                }
                AssistantMessageEvent::ToolCallDelta { .. } => order.push("args"),
                AssistantMessageEvent::Done { .. } => order.push("done"),
                _ => {}
            }
        }
        server.join().unwrap();
        let result = stream.result().await.unwrap();
        assert_eq!(result.stop_reason, StopReason::ToolUse, "{:?}", result.error_message);
        assert_eq!(order, ["text", "thinking", "args", "args", "done"]);
        assert!(matches!(&result.content[0], AssistantContent::Text(t) if t.text.units() == [0xd800]));
        assert!(matches!(&result.content[1], AssistantContent::Thinking(t) if t.thinking.units() == [0xdfff]));
        let AssistantContent::ToolCall(call) = &result.content[2] else {
            panic!("missing call")
        };
        assert_eq!(
            pi_js_compat::js_json::JsValue::Object(call.arguments.clone()),
            parse_lossless(r#"{"\ud800":[{"nested":"\udfff","id":"\ud800","name":"\udfff","type":"\ud800"}]}"#)
                .unwrap()
        );
        let module = format!("packages/ai/src/api/{api}.ts");
        let mut source = oracle::TsOracle::spawn("lossless-ingress-oracle.ts", &module);
        let expected = source.request(&serde_json::json!({"model":oracle_model,"sse":events,"apiKey":api_key}));
        let expected = parse_lossless(expected["wire"].as_str().unwrap()).unwrap();
        let actual = parse_lossless(serde_json::to_string(&result).unwrap()).unwrap();
        let source_types: Vec<_> = expected
            .get("events")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event.get("type").unwrap().clone())
            .collect();
        assert_eq!(emitted_types, source_types, "{api}");
        assert_eq!(
            actual.get("content"),
            expected.get("result").unwrap().get("content"),
            "{api}"
        );
    }
}

fn fake_codex_token() -> String {
    let payload = pi_js_compat::base64::btoa(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"fake"}}"#);
    format!("fake.{payload}.fake")
}

fn test_model(api: &str, url: &str) -> Model {
    serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":api,"provider":"openai","baseUrl":url,"reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap()
}

#[tokio::test]
async fn protocol_metadata_rejection_is_an_observable_stream_error() {
    for api in ["openai-completions", "mistral-conversations", "openai-responses"] {
        let event = if api == "openai-responses" {
            r#"data: {"type":"response.output_item.added","item":{"type":"function_call","id":"\ud800","call_id":"c","name":"probe","arguments":"{}"}}

"#
        } else {
            r#"data: {"id":"\ud800","model":"probe","choices":[{"index":0,"finish_reason":"stop","delta":{"content":"visible"}}]}

"#
        };
        let (url, server) = serve_sse(event);
        let model = test_model(api, &url);
        let mut options = pi_ai::request::StreamOptions::default();
        options.request.api_key = Some("fake".into());
        options.request.max_retries = Some(0.0);
        let context = Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        };
        let mut stream = match api {
            "openai-completions" => pi_ai::api::openai_completions::stream(model, context, options),
            "mistral-conversations" => pi_ai::api::mistral_conversations::stream(
                model,
                context,
                pi_ai::api::mistral_conversations::MistralOptions {
                    stream: options,
                    ..Default::default()
                },
            ),
            _ => pi_ai::api::openai_responses::stream(model, context, options),
        };
        let mut errors = 0;
        while let Some(event) = stream.next().await {
            if matches!(event.unwrap(), AssistantMessageEvent::Error { .. }) {
                errors += 1;
            }
        }
        server.join().unwrap();
        let result = stream.result().await.unwrap();
        assert_eq!(errors, 1, "{api}");
        assert_eq!(result.stop_reason, StopReason::Error, "{api}");
        assert!(result.error_message.unwrap().contains("Provider metadata id"), "{api}");
        assert!(result.content.is_empty(), "{api}");
    }
}

#[tokio::test]
async fn cancellation_after_raw_delta_retains_partial_and_aborted_status() {
    let (release, gate) = std::sync::mpsc::channel();
    let (url, server) = serve_gated_sse(
        "data: {\"choices\":[{\"delta\":{\"content\":\"\\ud800\"}}]}\n\n",
        Some(gate),
    );
    let cancel_controller = pi_js_compat::abort::AbortController::new();
    let cancel = cancel_controller.signal();
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.api_key = Some("fake".into());
    options.request.signal = Some(cancel.clone());
    let mut stream = pi_ai::api::openai_completions::stream(
        test_model("openai-completions", &url),
        Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        options,
    );
    let mut raw_delta_seen = false;
    while let Some(event) = stream.next().await {
        if let AssistantMessageEvent::TextDelta { delta, .. } = event.unwrap() {
            assert_eq!(delta.units(), &[0xd800]);
            raw_delta_seen = true;
            cancel_controller.abort(None);
            release.send(()).unwrap();
        }
    }
    server.join().unwrap();
    let result = stream.result().await.unwrap();
    assert!(raw_delta_seen);
    assert_eq!(result.stop_reason, StopReason::Aborted);
    assert!(matches!(&result.content[0], AssistantContent::Text(text) if text.text.units() == [0xd800]));
}

fn response_sse() -> String {
    [
        r#"{"type":"response.output_item.added","output_index":9,"item":{"type":"web_search_call"}}"#,
        r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"m"}}"#,
        r#"{"type":"response.output_text.delta","output_index":0,"delta":"\ud800"}"#,
        r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"m","content":[{"type":"output_text","text":"\ud800\ue200cite\ue202turn0search0\ue201"}]}}"#,
        r#"{"type":"response.output_item.added","output_index":1,"item":{"type":"reasoning","id":"r"}}"#,
        r#"{"type":"response.reasoning_summary_text.delta","output_index":1,"delta":"\udfff"}"#,
        r#"{"type":"response.output_item.done","output_index":1,"item":{"type":"reasoning","id":"r","summary":[{"text":"\udfff","id":"\ud800","name":"\udfff","type":"\ud800"}]}}"#,
        r#"{"type":"response.output_item.added","output_index":2,"item":{"type":"function_call","id":"f","call_id":"c","name":"probe","arguments":""}}"#,
        r#"{"type":"response.function_call_arguments.delta","output_index":2,"delta":"{\"\ud800\":[{\"nested\":\""}"#,
        r#"{"type":"response.function_call_arguments.delta","output_index":2,"delta":"\udfff\",\"id\":\"\ud800\",\"name\":\"\udfff\",\"type\":\"\ud800\"}]}"}"#,
        r#"{"type":"response.output_item.done","output_index":2,"item":{"type":"function_call","id":"f","call_id":"c","name":"probe","arguments":"{\"\ud800\":[{\"nested\":\"\udfff\",\"id\":\"\ud800\",\"name\":\"\udfff\",\"type\":\"\ud800\"}]}"}}"#,
        r#"{"type":"response.completed","response":{"status":"completed","output":[]}}"#,
    ].map(|event| format!("data: {event}\n\n")).concat()
}

#[tokio::test]
async fn codex_websocket_events_keep_native_text_and_argument_units() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
        let Message::Text(request) = socket.next().await.unwrap().unwrap() else {
            panic!("missing create frame")
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request).unwrap()["type"],
            "response.create"
        );
        for line in response_sse().lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                socket.send(Message::Text(data.to_owned().into())).await.unwrap();
            }
        }
        // Client owns termination after the terminal event.
        while let Some(message) = socket.next().await {
            if matches!(message, Ok(Message::Close(_)) | Err(_)) {
                break;
            }
        }
    });
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.api_key = Some((fake_codex_token()).into());
    options.request.max_retries = Some(0.0);
    options.transport = Some(pi_ai::types::Transport::Websocket);
    let mut stream = pi_ai::api::openai_codex_responses::stream(
        test_model("openai-codex-responses", &format!("http://{address}/v1")),
        Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        pi_ai::api::openai_codex_responses::OpenAICodexResponsesOptions {
            stream: options,
            ..Default::default()
        },
    );
    let mut deltas = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(event) = stream.next().await {
            match event.unwrap() {
                AssistantMessageEvent::TextDelta { delta, .. } => {
                    assert_eq!(delta.units(), &[0xd800]);
                    deltas.push("text");
                }
                AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                    assert_eq!(delta.units(), &[0xdfff]);
                    deltas.push("thinking");
                }
                AssistantMessageEvent::ToolCallDelta { .. } => deltas.push("args"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    let result = stream.result().await.unwrap();
    assert_eq!(result.stop_reason, StopReason::ToolUse, "{:?}", result.error_message);
    assert_eq!(deltas, ["text", "thinking", "args", "args"]);
    assert!(matches!(&result.content[0], AssistantContent::Text(text) if text.text.units() == [0xd800]));
    let AssistantContent::ToolCall(call) = &result.content[2] else {
        panic!("missing call")
    };
    assert_eq!(
        pi_js_compat::js_json::JsValue::Object(call.arguments.clone()),
        parse_lossless(r#"{"\ud800":[{"nested":"\udfff","id":"\ud800","name":"\udfff","type":"\ud800"}]}"#).unwrap()
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}
