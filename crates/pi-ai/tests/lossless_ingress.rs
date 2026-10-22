use pi_ai::types::{AssistantContent, AssistantMessageEvent, Context, Model, StopReason};
use pi_js_compat::js_json::parse_lossless;
use std::io::{Read, Write};

fn serve_sse(events: &str) -> (String, std::thread::JoinHandle<()>) {
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
        socket.write_all(b"0\r\n\r\n").unwrap();
    });
    (format!("http://{address}/v1"), task)
}

#[tokio::test]
async fn completions_and_mistral_one_byte_sse_preserve_raw_content() {
    for api in ["openai-completions", "mistral-conversations"] {
        let events = concat!(
            "data: {\"id\":\"msg\",\"choices\":[{\"delta\":{\"content\":\"t\\ud800\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":[{\"type\":\"thinking\",\"thinking\":[{\"text\":\"h\\udfff\"}]}],\"reasoning_content\":\"h\\udfff\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call\",\"function\":{\"name\":\"probe\",\"arguments\":\"{\\\"\\ud800\\\":[{\\\"nested\\\":\\\"\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call\",\"function\":{\"arguments\":\"\\udfff\\\"}]}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n"
        );
        let (url, server) = serve_sse(events);
        let model: Model = serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":api,"provider":if api.starts_with("mistral") {"mistral"} else {"openai"},"baseUrl":url,"reasoning":true,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap();
        let mut options = pi_ai::request::StreamOptions::default();
        options.request.api_key = Some("fake-loopback-only".into());
        options.request.max_retries = Some(0.0);
        let context = Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        };
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
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while let Some(event) = stream.next().await {
                match event.unwrap() {
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
            parse_lossless(r#"{"\ud800":[{"nested":"\udfff"}]}"#).unwrap()
        );
    }
}

#[tokio::test]
async fn responses_one_byte_sse_preserves_deltas_and_done_records() {
    let events = [
        r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"m"}}"#,
        r#"{"type":"response.output_text.delta","output_index":0,"delta":"\ud800"}"#,
        r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"m","content":[{"type":"output_text","text":"\ud800"}]}}"#,
        r#"{"type":"response.output_item.added","output_index":1,"item":{"type":"reasoning","id":"r"}}"#,
        r#"{"type":"response.reasoning_summary_text.delta","output_index":1,"delta":"\udfff"}"#,
        r#"{"type":"response.output_item.done","output_index":1,"item":{"type":"reasoning","id":"r","summary":[{"text":"\udfff"}]}}"#,
        r#"{"type":"response.output_item.added","output_index":2,"item":{"type":"function_call","id":"f","call_id":"c","name":"probe","arguments":""}}"#,
        r#"{"type":"response.function_call_arguments.delta","output_index":2,"delta":"{\"\ud800\":[{\"nested\":\""}"#,
        r#"{"type":"response.function_call_arguments.delta","output_index":2,"delta":"\udfff\"}]}"}"#,
        r#"{"type":"response.output_item.done","output_index":2,"item":{"type":"function_call","id":"f","call_id":"c","name":"probe","arguments":"{\"\ud800\":[{\"nested\":\"\udfff\"}]}"}}"#,
        r#"{"type":"response.completed","response":{"status":"completed","output":[]}}"#,
    ].map(|event| format!("data: {event}\n\n")).concat();
    let (url, server) = serve_sse(&events);
    let model: Model = serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":"openai-responses","provider":"openai","baseUrl":url,"reasoning":true,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap();
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.api_key = Some("fake-loopback-only".into());
    options.request.max_retries = Some(0.0);
    let mut stream = pi_ai::api::openai_responses::stream(
        model,
        Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        options,
    );
    let mut order = Vec::new();
    while let Some(event) = stream.next().await {
        match event.unwrap() {
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
        parse_lossless(r#"{"\ud800":[{"nested":"\udfff"}]}"#).unwrap()
    );
}
