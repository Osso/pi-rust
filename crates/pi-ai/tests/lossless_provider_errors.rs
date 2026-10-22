use pi_ai::types::{AssistantMessageEvent, Context, Model, StopReason};
use pi_js_compat::js_json::{JsString, parse_lossless};
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
