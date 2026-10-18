#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/wire.rs"]
mod wire;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_protocol::{
    AssistantContent, AssistantTranscriptItem, AssistantTranscriptState, AttachCommand, ClientHello, ClientMessage,
    Command, CreateCommand, CreateCommandInput, DetachCommand, FrameDecoder, JsonValue, ListCommand, PROTOCOL_VERSION,
    PromptCommand, RequestEnvelope, SetThinkingCommand, SteerCommand, TextContent, ThinkingLevel, ToolContent,
    ToolTranscriptItem, ToolTranscriptState, TranscriptDeltaKind, TranscriptItem, TranscriptProgress, Usage, UsageCost,
    UsageInput, encode_client_message,
};
use pi_server::testing::TestServerService;
use pi_server::transports::unix::{UnixServerOptions, create_unix_server};
use pi_server::types::PiSessionRuntime;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const SETTLE_MS: u64 = 150;

enum RuntimeAction {
    FinishPrompt,
    Abort,
    EmitSnapshot,
    EmitProgress(Box<TranscriptProgress>),
}

enum Step {
    Send(Vec<u8>),
    Runtime { id: &'static str, action: RuntimeAction },
}

fn command_frame(command: Command) -> Vec<u8> {
    let envelope = RequestEnvelope::new(format!("request-{}", command.as_str()), command).expect("request");
    encode_client_message(&ClientMessage::Request(envelope), None).expect("client frame")
}

fn hello_frame() -> Vec<u8> {
    encode_client_message(&ClientMessage::Hello(ClientHello::new(PROTOCOL_VERSION)), None).expect("hello frame")
}

fn usage() -> Usage {
    Usage::new(UsageInput {
        input: 1,
        output: 2,
        cache_read: 3,
        cache_write: 4,
        reasoning: Some(5),
        total_tokens: 15,
        cost: UsageCost::new(0.1, 0.2, 0.3, 0.4, 1.0).expect("usage cost"),
    })
}

fn usage_json() -> Value {
    json!({
        "input": 1,
        "output": 2,
        "cacheRead": 3,
        "cacheWrite": 4,
        "reasoning": 5,
        "totalTokens": 15,
        "cost": { "input": 0.1, "output": 0.2, "cacheRead": 0.3, "cacheWrite": 0.4, "total": 1 }
    })
}

fn model_ref() -> pi_protocol::ModelRef {
    pi_protocol::ModelRef::new("test".into(), "small".into()).expect("test model reference")
}

fn assistant_item(state: AssistantTranscriptState, content: &str) -> AssistantTranscriptItem {
    AssistantTranscriptItem::new(pi_protocol::AssistantTranscriptItemInput {
        id: "assistant-progress".into(),
        content: vec![AssistantContent::Text(TextContent::new(content.into()))],
        model: model_ref(),
        response_model: None,
        usage: None,
        timestamp: 7,
        state,
    })
    .expect("assistant progress item")
}

fn tool_item(state: ToolTranscriptState, details: Option<JsonValue>, usage: Option<Usage>) -> ToolTranscriptItem {
    ToolTranscriptItem::new(pi_protocol::ToolTranscriptItemInput {
        id: "tool-progress".into(),
        tool_call_id: "call-progress".into(),
        tool_name: "read".into(),
        input: JsonValue::Object(vec![("path".into(), JsonValue::String("README.md".into()))]),
        content: vec![ToolContent::Text(TextContent::new("result".into()))],
        details,
        usage,
        timestamp: 8,
        state,
    })
    .expect("tool progress item")
}

fn progress_steps() -> Vec<Step> {
    let streaming = assistant_item(AssistantTranscriptState::Streaming, "hello");
    let error = assistant_item(
        AssistantTranscriptState::Error {
            error_message: Some("boom".into()),
        },
        "",
    );
    let aborted = assistant_item(AssistantTranscriptState::Aborted { error_message: None }, "");
    let running = tool_item(ToolTranscriptState::Running, None, None);
    let complete = tool_item(
        ToolTranscriptState::Complete,
        Some(JsonValue::Object(vec![("lines".into(), JsonValue::Number(2.0))])),
        Some(usage()),
    );
    let failed = tool_item(ToolTranscriptState::Error, None, None);
    let session = "session-1";
    let progress = |progress: TranscriptProgress| Step::Runtime {
        id: session,
        action: RuntimeAction::EmitProgress(Box::new(progress)),
    };
    vec![
        progress(TranscriptProgress::item_started(TranscriptItem::Assistant(streaming))),
        progress(
            TranscriptProgress::assistant_delta(
                "assistant-progress".into(),
                0,
                TranscriptDeltaKind::Text,
                "hello".into(),
            )
            .expect("assistant delta"),
        ),
        progress(TranscriptProgress::item_updated(TranscriptItem::Tool(running)).expect("running tool update")),
        progress(TranscriptProgress::item_finished(TranscriptItem::Tool(complete)).expect("complete tool")),
        progress(TranscriptProgress::item_finished(TranscriptItem::Tool(failed)).expect("failed tool")),
        progress(TranscriptProgress::item_finished(TranscriptItem::Assistant(error)).expect("error assistant")),
        progress(TranscriptProgress::item_finished(TranscriptItem::Assistant(aborted)).expect("aborted assistant")),
        Step::Runtime {
            id: session,
            action: RuntimeAction::EmitSnapshot,
        },
    ]
}

fn progress_json_steps() -> Vec<Value> {
    vec![
        json!({ "runtime": { "id": "session-1", "action": "emitProgress", "progress": {
            "type": "item_started",
            "item": {
                "id": "assistant-progress",
                "role": "assistant",
                "content": [{ "type": "text", "text": "hello" }],
                "model": { "provider": "test", "id": "small" },
                "timestamp": 7,
                "status": "streaming"
            }
        }}}),
        json!({ "runtime": { "id": "session-1", "action": "emitProgress", "progress": {
            "type": "assistant_delta",
            "messageId": "assistant-progress",
            "contentIndex": 0,
            "kind": "text",
            "delta": "hello"
        }}}),
        json!({ "runtime": { "id": "session-1", "action": "emitProgress", "progress": {
            "type": "item_updated",
            "item": {
                "id": "tool-progress",
                "role": "tool",
                "toolCallId": "call-progress",
                "toolName": "read",
                "input": { "path": "README.md" },
                "content": [{ "type": "text", "text": "result" }],
                "timestamp": 8,
                "status": "running",
                "isError": false
            }
        }}}),
        json!({ "runtime": { "id": "session-1", "action": "emitProgress", "progress": {
            "type": "item_finished",
            "item": {
                "id": "tool-progress",
                "role": "tool",
                "toolCallId": "call-progress",
                "toolName": "read",
                "input": { "path": "README.md" },
                "content": [{ "type": "text", "text": "result" }],
                "details": { "lines": 2 },
                "usage": usage_json(),
                "timestamp": 8,
                "status": "complete",
                "isError": false
            }
        }}}),
        json!({ "runtime": { "id": "session-1", "action": "emitProgress", "progress": {
            "type": "item_finished",
            "item": {
                "id": "tool-progress",
                "role": "tool",
                "toolCallId": "call-progress",
                "toolName": "read",
                "input": { "path": "README.md" },
                "content": [{ "type": "text", "text": "result" }],
                "timestamp": 8,
                "status": "error",
                "isError": true
            }
        }}}),
        json!({ "runtime": { "id": "session-1", "action": "emitProgress", "progress": {
            "type": "item_finished",
            "item": {
                "id": "assistant-progress",
                "role": "assistant",
                "content": [{ "type": "text", "text": "" }],
                "model": { "provider": "test", "id": "small" },
                "timestamp": 7,
                "status": "error",
                "stopReason": "error",
                "errorMessage": "boom"
            }
        }}}),
        json!({ "runtime": { "id": "session-1", "action": "emitProgress", "progress": {
            "type": "item_finished",
            "item": {
                "id": "assistant-progress",
                "role": "assistant",
                "content": [{ "type": "text", "text": "" }],
                "model": { "provider": "test", "id": "small" },
                "timestamp": 7,
                "status": "aborted",
                "stopReason": "aborted"
            }
        }}}),
        json!({ "runtime": { "id": "session-1", "action": "emitSnapshot" } }),
    ]
}

fn scripted_steps() -> (Vec<Step>, Vec<Value>) {
    let mut steps = vec![
        Step::Send(hello_frame()),
        Step::Send(command_frame(Command::List(ListCommand::new()))),
        Step::Send(command_frame(Command::Attach(
            AttachCommand::new("session-1".into()).expect("attach"),
        ))),
        Step::Send(command_frame(Command::SetThinking(
            SetThinkingCommand::new("session-1".into(), ThinkingLevel::Medium).expect("set thinking"),
        ))),
        Step::Send(command_frame(Command::Prompt(
            PromptCommand::new("session-1".into(), "hello".into()).expect("prompt"),
        ))),
    ];
    let mut oracle_steps = vec![
        json!({ "sendHex": wire::hex(&hello_frame()) }),
        json!({ "sendHex": wire::hex(&command_frame(Command::List(ListCommand::new()))) }),
        json!({ "sendHex": wire::hex(&command_frame(Command::Attach(
            AttachCommand::new("session-1".into()).expect("attach")
        ))) }),
        json!({ "sendHex": wire::hex(&command_frame(Command::SetThinking(
            SetThinkingCommand::new("session-1".into(), ThinkingLevel::Medium).expect("set thinking")
        ))) }),
        json!({ "sendHex": wire::hex(&command_frame(Command::Prompt(
            PromptCommand::new("session-1".into(), "hello".into()).expect("prompt")
        ))) }),
    ];
    steps.extend(progress_steps());
    oracle_steps.extend(progress_json_steps());
    steps.push(Step::Runtime {
        id: "session-1",
        action: RuntimeAction::FinishPrompt,
    });
    oracle_steps.push(json!({ "runtime": { "id": "session-1", "action": "finishPrompt" } }));
    steps.push(Step::Send(command_frame(Command::Prompt(
        PromptCommand::new("session-1".into(), "second".into()).expect("prompt"),
    ))));
    oracle_steps.push(json!({ "sendHex": wire::hex(&command_frame(Command::Prompt(
        PromptCommand::new("session-1".into(), "second".into()).expect("prompt")
    ))) }));
    steps.push(Step::Send(command_frame(Command::Steer(
        SteerCommand::new("session-1".into(), "adjust".into()).expect("steer"),
    ))));
    oracle_steps.push(json!({ "sendHex": wire::hex(&command_frame(Command::Steer(
        SteerCommand::new("session-1".into(), "adjust".into()).expect("steer")
    ))) }));
    steps.push(Step::Runtime {
        id: "session-1",
        action: RuntimeAction::Abort,
    });
    oracle_steps.push(json!({ "runtime": { "id": "session-1", "action": "abort" } }));
    steps.push(Step::Send(command_frame(Command::Detach(
        DetachCommand::new("session-1".into()).expect("detach"),
    ))));
    oracle_steps.push(json!({ "sendHex": wire::hex(&command_frame(Command::Detach(
        DetachCommand::new("session-1".into()).expect("detach")
    ))) }));
    steps.push(Step::Send(command_frame(Command::Create(
        CreateCommand::new(CreateCommandInput {
            cwd: None,
            name: Some("Created".into()),
            model: None,
            thinking_level: None,
        })
        .expect("create"),
    ))));
    oracle_steps.push(json!({ "sendHex": wire::hex(&command_frame(Command::Create(
        CreateCommand::new(CreateCommandInput {
            cwd: None,
            name: Some("Created".into()),
            model: None,
            thinking_level: None,
        })
        .expect("create")
    ))) }));
    steps.push(Step::Send(command_frame(Command::List(ListCommand::new()))));
    oracle_steps.push(json!({ "sendHex": wire::hex(&command_frame(Command::List(ListCommand::new()))) }));
    steps.push(Step::Send(command_frame(Command::Attach(
        AttachCommand::new("session-2".into()).expect("attach"),
    ))));
    oracle_steps.push(json!({ "sendHex": wire::hex(&command_frame(Command::Attach(
        AttachCommand::new("session-2".into()).expect("attach")
    ))) }));
    steps.push(Step::Send(command_frame(Command::Detach(
        DetachCommand::new("session-2".into()).expect("detach"),
    ))));
    oracle_steps.push(json!({ "sendHex": wire::hex(&command_frame(Command::Detach(
        DetachCommand::new("session-2".into()).expect("detach")
    ))) }));
    steps.push(Step::Send(command_frame(Command::Attach(
        AttachCommand::new("missing-session".into()).expect("attach"),
    ))));
    oracle_steps.push(json!({ "sendHex": wire::hex(&command_frame(Command::Attach(
        AttachCommand::new("missing-session".into()).expect("attach")
    ))) }));
    let unsupported_hello = encode_client_message(&ClientMessage::Hello(ClientHello::new(PROTOCOL_VERSION + 1)), None)
        .expect("unsupported hello frame");
    steps.push(Step::Send(unsupported_hello.clone()));
    oracle_steps.push(json!({ "sendHex": wire::hex(&unsupported_hello) }));
    (steps, oracle_steps)
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

async fn run_runtime_action(service: &TestServerService, id: &str, action: RuntimeAction) {
    let runtime = service.latest_runtime(id);
    match action {
        RuntimeAction::FinishPrompt => runtime.finish_prompt(),
        RuntimeAction::Abort => runtime.abort().await.expect("runtime abort"),
        RuntimeAction::EmitSnapshot => runtime.emit_snapshot(),
        RuntimeAction::EmitProgress(progress) => runtime.emit_progress(*progress),
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
            service.clone(),
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

    let (steps, oracle_steps) = scripted_steps();
    let mut rust_steps: Vec<Vec<String>> = Vec::new();
    for step in steps {
        let index = frames.lock().expect("frames mutex").len();
        match step {
            Step::Send(frame) => writer.write_all(&frame).await.expect("client frame must send"),
            Step::Runtime { id, action } => run_runtime_action(&service, id, action).await,
        }
        let step_frames = settle(&frames, index).await;
        rust_steps.push(step_frames.iter().map(|frame| wire::frame_hex(frame)).collect());
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
        let ts_frames: Vec<String> = ts
            .as_array()
            .expect("frame array")
            .iter()
            .map(|frame| wire::frame_hex(&wire::decode_hex(frame.as_str().expect("frame hex"))))
            .collect();
        assert_eq!(&ts_frames, rust, "step {index} differs");
    }

    reader_task.abort();
    drop(writer);
    server.close().await.expect("server must close");
}
