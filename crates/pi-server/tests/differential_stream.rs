#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_protocol::{
    AttachCommand, CborValue, ClientHello, ClientMessage, Command, CreateCommand, CreateCommandInput, DetachCommand,
    FrameDecoder, ListCommand, PROTOCOL_VERSION, RequestEnvelope, SetThinkingCommand, ThinkingLevel,
    encode_client_message,
};
use pi_server::testing::TestServerService;
use pi_server::transports::unix::{UnixServerOptions, create_unix_server};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const SETTLE_MS: u64 = 150;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).expect("hex digits"))
        .collect()
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

fn normalize_value(value: &CborValue, key: Option<&str>) -> CborValue {
    match value {
        CborValue::Text(text) => {
            if key == Some("connectionId") {
                CborValue::Text("<connection>".into())
            } else if is_uuid(text) {
                CborValue::Text("<uuid>".into())
            } else {
                CborValue::Text(text.clone())
            }
        }
        CborValue::Array(values) => CborValue::Array(values.iter().map(|value| normalize_value(value, None)).collect()),
        CborValue::Map(entries) => {
            let mut entries: Vec<(String, CborValue)> = entries
                .iter()
                .map(|(entry_key, entry)| (entry_key.clone(), normalize_value(entry, Some(entry_key))))
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            CborValue::Map(entries)
        }
        value => value.clone(),
    }
}

fn canonical_payload(payload: &[u8]) -> String {
    let value = pi_protocol::decode_cbor(payload, None).expect("payload must decode");
    hex(&pi_protocol::encode_cbor(&normalize_value(&value, None), None).expect("canonical payload"))
}

fn canonical_frame(frame_hex: &str) -> String {
    let frame = decode_hex(frame_hex);
    canonical_payload(&frame[4..])
}

fn command_frame(command: Command) -> Vec<u8> {
    let envelope = RequestEnvelope::new(format!("request-{}", command.as_str()), command).expect("request");
    encode_client_message(&ClientMessage::Request(envelope), None).expect("client frame")
}

fn scripted_frames() -> Vec<Vec<u8>> {
    vec![
        encode_client_message(&ClientMessage::Hello(ClientHello::new(PROTOCOL_VERSION)), None).expect("hello frame"),
        command_frame(Command::List(ListCommand::new())),
        command_frame(Command::Attach(AttachCommand::new("session-1".into()).expect("attach"))),
        command_frame(Command::SetThinking(
            SetThinkingCommand::new("session-1".into(), ThinkingLevel::Medium).expect("set thinking"),
        )),
        command_frame(Command::Detach(DetachCommand::new("session-1".into()).expect("detach"))),
        command_frame(Command::Create(
            CreateCommand::new(CreateCommandInput {
                cwd: None,
                name: Some("Created".into()),
                model: None,
                thinking_level: None,
            })
            .expect("create"),
        )),
        command_frame(Command::List(ListCommand::new())),
        command_frame(Command::Attach(AttachCommand::new("session-2".into()).expect("attach"))),
        command_frame(Command::Detach(DetachCommand::new("session-2".into()).expect("detach"))),
    ]
}

async fn settle(frames: &Arc<Mutex<Vec<Vec<u8>>>>, index: usize) -> Vec<Vec<u8>> {
    let mut last_length = usize::MAX;
    loop {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let length = frames.lock().expect("frames mutex").len();
        if length == last_length {
            return frames.lock().expect("frames mutex")[index..].to_vec();
        }
        last_length = length;
        tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;
        let settled = frames.lock().expect("frames mutex").len();
        if settled == length {
            return frames.lock().expect("frames mutex")[index..].to_vec();
        }
    }
}

#[tokio::test]
async fn scripted_client_streams_produce_identical_server_frames() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("server.sock").to_string_lossy().into_owned();
    let service = Arc::new(TestServerService::new());
    service.seed("session-1");
    service.seed("session-2");
    let server = Arc::new(
        create_unix_server(
            service,
            UnixServerOptions {
                path: path.clone(),
                server_id: Some("server-1".into()),
                ..UnixServerOptions::default()
            },
        )
        .expect("server options"),
    );
    server.start().await.expect("server must start");

    let stream = UnixStream::connect(&path).await.expect("client must connect");
    let (reader, mut writer) = stream.into_split();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let reader_frames = frames.clone();
    let reader_task = tokio::spawn(async move {
        let mut reader = reader;
        let mut decoder = FrameDecoder::new(None);
        let mut buffer = [0_u8; 8 * 1024];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) => return,
                Ok(bytes) => {
                    let decoded = decoder.push(&buffer[..bytes]).expect("frames must decode");
                    reader_frames.lock().expect("frames mutex").extend(decoded);
                }
                Err(_) => return,
            }
        }
    });

    let mut rust_steps: Vec<Vec<String>> = Vec::new();
    let mut oracle_steps = Vec::new();
    for frame in scripted_frames() {
        let index = frames.lock().expect("frames mutex").len();
        writer.write_all(&frame).await.expect("client frame must send");
        let step_frames = settle(&frames, index).await;
        let mut canonical: Vec<String> = step_frames.iter().map(|payload| canonical_payload(payload)).collect();
        canonical.sort();
        rust_steps.push(canonical);
        oracle_steps.push(json!({ "sendHex": hex(&frame) }));
    }

    let mut ts_oracle = oracle::TsOracle::spawn(
        "server-stream-oracle.ts",
        "packages/server/src/transports/unix/preset.ts",
    );
    let response = ts_oracle.request(&json!({
        "id": 1,
        "serverId": "server-1",
        "settleMs": SETTLE_MS,
        "steps": oracle_steps,
    }));
    assert_eq!(response["ok"], true, "stream oracle failed: {response}");
    let ts_steps = response["output"]["steps"].as_array().expect("steps array");
    assert_eq!(ts_steps.len(), rust_steps.len());
    for (index, (rust, ts)) in rust_steps.iter().zip(ts_steps).enumerate() {
        let mut ts_frames: Vec<String> = ts
            .as_array()
            .expect("frame array")
            .iter()
            .map(|frame| canonical_frame(frame.as_str().expect("frame hex")))
            .collect();
        ts_frames.sort();
        assert_eq!(&ts_frames, rust, "step {index} differs");
    }

    reader_task.abort();
    drop(writer);
    server.close().await.expect("server must close");
}
