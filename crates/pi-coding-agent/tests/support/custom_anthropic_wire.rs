use super::*;
use pi_ai::request::SimpleStreamOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::Duration;

struct WireServer {
    url: String,
    thread: JoinHandle<Vec<Value>>,
}
impl WireServer {
    fn start(status: u16, body: String, truncated: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let mut requests = vec![];
            for _ in 0..2 {
                let deadline = std::time::Instant::now() + Duration::from_secs(15);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(std::time::Instant::now() < deadline, "HTTP fixture request timed out");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("HTTP fixture accept: {error}"),
                    }
                };
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                requests.push(read_request(&socket));
                let length = body.len() + if truncated { 50 } else { 0 };
                write!(socket, "HTTP/1.1 {status} Fixture\r\nContent-Type: {}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n", if status == 200 { "text/event-stream" } else { "application/json" }).unwrap();
                socket.write_all(body.as_bytes()).unwrap();
                socket.flush().unwrap();
            }
            requests
        });
        Self { url, thread }
    }
}
fn read_request(socket: &TcpStream) -> Value {
    let mut reader = BufReader::new(socket);
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    let mut headers = serde_json::Map::new();
    let mut raw = first.clone();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        raw.push_str(&line);
        if line == "\r\n" {
            break;
        }
        let (name, value) = line.split_once(':').expect("header");
        headers.insert(name.to_lowercase(), json!(value.trim()));
    }
    let length: usize = headers["content-length"].as_str().unwrap().parse().unwrap();
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).unwrap();
    raw.push_str(std::str::from_utf8(&bytes).unwrap());
    json!({"raw":raw,"request":first.trim(),"headers":headers,"body":serde_json::from_slice::<Value>(&bytes).unwrap()})
}
fn wire_events() -> Vec<Value> {
    vec![
        json!({"type":"message_start","message":{"id":"msg_fixture","type":"message","role":"assistant","model":"claude-opus-4-5","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":11,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello 😀"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"thinking_delta","thinking":"reason"}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"signature_delta","signature":"sig"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call_fixture","name":"Read","input":{}}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"\"a\"}"}}),
        json!({"type":"content_block_stop","index":2}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":7}}),
        json!({"type":"message_stop"}),
    ]
}
fn sse(events: &[Value]) -> String {
    events
        .iter()
        .map(|e| format!("event: {}\ndata: {}\n\n", e["type"].as_str().unwrap(), e))
        .collect()
}
async fn compare_wire(key: &str, status: u16, body: String, truncated: bool) {
    let server = WireServer::start(status, body, truncated);
    let input = json!({"wire":true,"model":{"baseUrl":server.url},"options":{"apiKey":key,"reasoning":"high"},"context":{"messages":[{"role":"user","content":"hello","timestamp":0}],"systemPrompt":"fixture system","tools":[{"name":"read","description":"read","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}]}});
    let mut oracle = oracle::TsOracle::spawn(
        "example-custom-anthropic-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts",
    );
    let expected = oracle.request(&input);
    assert_eq!(
        expected["sdkVersion"], "0.91.1",
        "wire oracle must use installed SDK, not fake client"
    );
    let provider = provider(&input, Arc::new(Mutex::new(vec![]))).await;
    let mut model = provider.get_models()[0].clone();
    model.base_url = server.url;
    let mut options = SimpleStreamOptions::default();
    options.stream.request.api_key = Some(key.into());
    options.stream.request.signal = Some(CancellationToken::new());
    options.stream.request.fetch = Some(pi_ai::http::default_fetch());
    options.reasoning = Some(pi_ai::types::ThinkingLevel::High);
    let mut stream = provider.stream_simple(
        model,
        serde_json::from_value(input["context"].clone()).unwrap(),
        options,
    );
    let mut events = vec![];
    while let Some(event) = tokio::time::timeout(Duration::from_secs(15), stream.next())
        .await
        .unwrap()
    {
        events.push(serde_json::to_value(event.unwrap()).unwrap());
    }
    let requests = server.thread.join().unwrap();
    let proof = std::env::temp_dir();
    let auth = if key.contains("sk-ant-oat") { "oauth" } else { "api-key" };
    std::fs::write(proof.join(format!("wire-{auth}-{status}-truncated-{truncated}.json")), serde_json::to_vec_pretty(&json!({"sdkVersion":expected["sdkVersion"],"requests":requests,"sourceEvents":expected["events"],"nativeEvents":events})).unwrap()).unwrap();
    assert_eq!(
        events,
        expected["events"].as_array().unwrap().clone(),
        "actual SDK events"
    );
    assert_eq!(requests[0]["body"], requests[1]["body"]);
    assert_eq!(requests[0]["request"], requests[1]["request"]);
    let mut node = requests[0]["headers"].as_object().unwrap().clone();
    let mut rust = requests[1]["headers"].as_object().unwrap().clone();
    assert_eq!(rust.remove("x-stainless-runtime").unwrap(), "rust");
    assert_eq!(node.remove("x-stainless-runtime").unwrap(), "node");
    rust.remove("x-stainless-runtime-version").unwrap();
    node.remove("x-stainless-runtime-version").unwrap();
    // Four native transport defaults approved 2026-10-20; no broader header projection.
    for (name, value) in [
        ("connection", "keep-alive"),
        ("accept-encoding", "gzip, deflate"),
        ("accept-language", "*"),
        ("sec-fetch-mode", "cors"),
    ] {
        assert_eq!(node.remove(name).unwrap(), value);
        assert!(rust.remove(name).is_none(), "native transport changed: {name}");
    }
    assert_eq!(
        rust, node,
        "actual HTTP headers outside enumerated native runtime/transport deltas"
    );
    let last = events.last().unwrap();
    assert_eq!(
        serde_json::to_value(stream.result().await.unwrap()).unwrap(),
        *last.get("message").or_else(|| last.get("error")).unwrap()
    );
}
#[tokio::test]
async fn installed_sdk_wire_auth_body_and_events() {
    for key in ["FAKE-api-key", "sk-ant-oat-FAKE-token"] {
        compare_wire(key, 200, sse(&wire_events()), false).await;
    }
}
#[tokio::test]
async fn installed_sdk_wire_status_error_and_terminated_body() {
    compare_wire(
        "FAKE-api-key",
        400,
        json!({"type":"error","error":{"type":"invalid_request_error","message":"fixture rejected"}}).to_string(),
        false,
    )
    .await;
    compare_wire("FAKE-api-key", 200, sse(&wire_events()[..3]), true).await;
}
