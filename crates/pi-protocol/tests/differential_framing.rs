use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use pi_protocol::{FrameDecodeError, FrameDecoder};
use serde::Deserialize;
use serde_json::{Value, json};

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));

#[derive(Debug, Deserialize)]
struct SourceLock {
    worktree: PathBuf,
    commit: String,
}

struct TsOracle {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

enum Operation {
    Encode(Vec<u8>),
    Assert {
        frame: Vec<u8>,
        max_frame_length: Option<u32>,
    },
    Decoder {
        max_frame_length: Option<u32>,
        steps: Vec<DecoderStep>,
    },
}

enum DecoderStep {
    Push { chunk: Vec<u8>, mutate_fill: Option<u8> },
    End,
}

struct Fixture {
    name: String,
    operation: Operation,
}

impl TsOracle {
    fn spawn() -> Self {
        let source_lock = read_source_lock();
        verify_source_head(&source_lock);
        let mut child = spawn_oracle_process(&source_lock);
        let stdin = BufWriter::new(child.stdin.take().expect("oracle stdin must be piped"));
        let stdout = BufReader::new(child.stdout.take().expect("oracle stdout must be piped"));

        Self { child, stdin, stdout }
    }

    fn request(&mut self, request: &Value) -> Value {
        serde_json::to_writer(&mut self.stdin, request).expect("oracle request must serialize");
        self.stdin
            .write_all(b"\n")
            .expect("oracle request must be newline-delimited");
        self.stdin.flush().expect("oracle stdin must flush");

        let mut line = String::new();
        let bytes_read = self
            .stdout
            .read_line(&mut line)
            .expect("oracle response must be readable");
        assert_ne!(bytes_read, 0, "oracle must return one response per request");
        serde_json::from_str(&line).expect("oracle response must be valid JSON")
    }
}

impl Drop for TsOracle {
    fn drop(&mut self) {
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Fixture {
    fn request(&self, id: u64) -> Value {
        match &self.operation {
            Operation::Encode(payload) => json!({
                "id": id,
                "operation": "encode_frame",
                "payload_hex": hex_encode(payload),
            }),
            Operation::Assert {
                frame,
                max_frame_length,
            } => assert_request(id, frame, *max_frame_length),
            Operation::Decoder {
                max_frame_length,
                steps,
            } => decoder_request(id, *max_frame_length, steps),
        }
    }

    fn rust_response(&self, id: u64) -> Value {
        match &self.operation {
            Operation::Encode(payload) => encode_response(id, payload),
            Operation::Assert {
                frame,
                max_frame_length,
            } => assert_response(id, frame, *max_frame_length),
            Operation::Decoder {
                max_frame_length,
                steps,
            } => decoder_response(id, *max_frame_length, steps),
        }
    }
}

fn read_source_lock() -> SourceLock {
    serde_json::from_str(SOURCE_LOCK).expect("source lock must be valid JSON")
}

fn spawn_oracle_process(source_lock: &SourceLock) -> Child {
    let tsx = source_lock.worktree.join("node_modules/.bin/tsx");
    let tsconfig = source_lock.worktree.join("tsconfig.json");
    let framing_module = source_lock.worktree.join("packages/protocol/src/framing.ts");
    let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/framing-oracle.ts");

    Command::new(&tsx)
        .arg("--tsconfig")
        .arg(tsconfig)
        .arg(oracle)
        .arg(framing_module)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("worktree-local tsx must start the TypeScript framing oracle")
}

fn verify_source_head(source_lock: &SourceLock) {
    let output = Command::new("git")
        .arg("-C")
        .arg(&source_lock.worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git must verify the pinned TypeScript source HEAD");
    assert!(
        output.status.success(),
        "git rev-parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual_commit = String::from_utf8(output.stdout)
        .expect("git HEAD must be UTF-8")
        .trim()
        .to_owned();
    assert_eq!(
        actual_commit, source_lock.commit,
        "TypeScript source HEAD must equal docs/porting/source-lock.json"
    );
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
}

fn assert_request(id: u64, frame: &[u8], max_frame_length: Option<u32>) -> Value {
    let mut request = json!({
        "id": id,
        "operation": "assert_complete_frame",
        "frame_hex": hex_encode(frame),
    });
    if let Some(maximum) = max_frame_length {
        request["max_frame_length"] = json!(maximum);
    }
    request
}

fn decoder_request(id: u64, max_frame_length: Option<u32>, steps: &[DecoderStep]) -> Value {
    let mut request = json!({
        "id": id,
        "operation": "decoder_sequence",
        "steps": steps.iter().map(decoder_step_request).collect::<Vec<_>>(),
    });
    if let Some(maximum) = max_frame_length {
        request["max_frame_length"] = json!(maximum);
    }
    request
}

fn decoder_step_request(step: &DecoderStep) -> Value {
    match step {
        DecoderStep::Push { chunk, mutate_fill } => {
            let mut request = json!({
                "op": "push",
                "chunk_hex": hex_encode(chunk),
            });
            if let Some(fill) = mutate_fill {
                request["mutate_fill"] = json!(fill);
            }
            request
        }
        DecoderStep::End => json!({"op": "end"}),
    }
}

fn encode_response(id: u64, payload: &[u8]) -> Value {
    match pi_protocol::encode_frame(payload) {
        Ok(frame) => json!({
            "id": id,
            "ok": true,
            "output": {"frame_hex": hex_encode(&frame)},
        }),
        Err(error) => json!({
            "id": id,
            "ok": false,
            "error": {
                "name": "RangeError",
                "message": error.to_string(),
            },
        }),
    }
}

fn assert_response(id: u64, frame: &[u8], max_frame_length: Option<u32>) -> Value {
    match pi_protocol::assert_complete_frame(frame, max_frame_length) {
        Ok(()) => json!({"id": id, "ok": true, "output": null}),
        Err(error) => json!({
            "id": id,
            "ok": false,
            "error": {
                "name": "FrameError",
                "message": error.to_string(),
            },
        }),
    }
}

fn decoder_response(id: u64, max_frame_length: Option<u32>, steps: &[DecoderStep]) -> Value {
    let mut decoder = FrameDecoder::new(max_frame_length);
    let results = steps
        .iter()
        .map(|step| decoder_step_response(&mut decoder, step))
        .collect::<Vec<_>>();
    json!({"id": id, "ok": true, "output": {"results": results}})
}

fn decoder_step_response(decoder: &mut FrameDecoder, step: &DecoderStep) -> Value {
    match step {
        DecoderStep::Push { chunk, mutate_fill } => {
            let mut owned_chunk = chunk.clone();
            let result = decoder.push(&owned_chunk);
            if let Some(fill) = mutate_fill {
                owned_chunk.fill(*fill);
            }
            push_response(result)
        }
        DecoderStep::End => match decoder.end() {
            Ok(()) => json!({"ok": true}),
            Err(error) => error_response("FrameError", error.to_string()),
        },
    }
}

fn push_response(result: Result<Vec<Vec<u8>>, FrameDecodeError>) -> Value {
    match result {
        Ok(frames) => json!({
            "ok": true,
            "frames_hex": frames.iter().map(|frame| hex_encode(frame)).collect::<Vec<_>>(),
        }),
        Err(FrameDecodeError::Frame(error)) => error_response("FrameError", error.to_string()),
        Err(FrameDecodeError::Allocation) => error_response("FrameDecodeError", "Frame decoder allocation failed"),
    }
}

fn error_response(name: &str, message: impl Into<String>) -> Value {
    json!({
        "ok": false,
        "error": {
            "name": name,
            "message": message.into(),
        },
    })
}

fn encode_fixture(name: impl Into<String>, payload: Vec<u8>) -> Fixture {
    Fixture {
        name: name.into(),
        operation: Operation::Encode(payload),
    }
}

fn assert_fixture(name: impl Into<String>, frame: Vec<u8>, max_frame_length: Option<u32>) -> Fixture {
    Fixture {
        name: name.into(),
        operation: Operation::Assert {
            frame,
            max_frame_length,
        },
    }
}

fn decoder_fixture(name: impl Into<String>, max_frame_length: Option<u32>, steps: Vec<DecoderStep>) -> Fixture {
    Fixture {
        name: name.into(),
        operation: Operation::Decoder {
            max_frame_length,
            steps,
        },
    }
}

fn push_step(chunk: Vec<u8>) -> DecoderStep {
    DecoderStep::Push {
        chunk,
        mutate_fill: None,
    }
}

fn mutating_push_step(chunk: Vec<u8>, fill: u8) -> DecoderStep {
    DecoderStep::Push {
        chunk,
        mutate_fill: Some(fill),
    }
}

fn end_step() -> DecoderStep {
    DecoderStep::End
}

fn repeated_bytes(byte: u8, count: usize) -> Vec<u8> {
    vec![byte; count]
}

fn patterned_payload(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

fn frame_bytes(payload: &[u8]) -> Vec<u8> {
    let payload_length = u32::try_from(payload.len()).expect("fixture payload must fit u32");
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn concatenate(chunks: &[Vec<u8>]) -> Vec<u8> {
    let total_length = chunks.iter().map(Vec::len).sum();
    let mut output = Vec::with_capacity(total_length);
    for chunk in chunks {
        output.extend_from_slice(chunk);
    }
    output
}

fn stateless_fixtures() -> Vec<Fixture> {
    vec![
        encode_fixture("empty payload", Vec::new()),
        encode_fixture("ordinary payload", vec![0xaa, 0xbb, 0xcc]),
        encode_fixture("high-order header bytes", repeated_bytes(0x5a, 70_000)),
        assert_fixture("exact frame", vec![0, 0, 0, 2, 1, 2], Some(2)),
        assert_fixture("short header", vec![0, 0, 0], None),
        assert_fixture("partial payload", vec![0, 0, 0, 2, 1], None),
        assert_fixture("trailing bytes", vec![0, 0, 0, 1, 1, 2], None),
        assert_fixture("configured limit", vec![0, 0, 0, 3, 1, 2, 3], Some(2)),
        assert_fixture("default limit", vec![1, 0, 0, 1], None),
    ]
}

fn decoder_stream_fixtures() -> Vec<Fixture> {
    let wire = concatenate(&[frame_bytes(&[1, 2, 3]), frame_bytes(&[]), frame_bytes(&[4])]);
    let mut fragmented_steps = wire.iter().map(|byte| push_step(vec![*byte])).collect::<Vec<_>>();
    fragmented_steps.push(end_step());

    vec![
        decoder_fixture("decoder fragmented frames", None, fragmented_steps),
        decoder_fixture("decoder coalesced frames", None, vec![push_step(wire), end_step()]),
        decoder_fixture(
            "decoder empty chunk and stream",
            None,
            vec![push_step(Vec::new()), end_step()],
        ),
        decoder_fixture(
            "decoder input aliasing",
            None,
            vec![mutating_push_step(frame_bytes(&[1, 2, 3]), 9), end_step()],
        ),
    ]
}

fn decoder_boundary_fixtures() -> Vec<Fixture> {
    [
        (65_536, "exact block boundary"),
        (65_537, "second block byte"),
        (70_000, "multi-block payload"),
    ]
    .into_iter()
    .map(|(payload_length, label)| {
        let payload = patterned_payload(payload_length);
        let wire = frame_bytes(&payload);
        let second_split = wire.len().min(65_541);
        decoder_fixture(
            format!("decoder {label}"),
            None,
            vec![
                push_step(wire[..101].to_vec()),
                push_step(wire[101..second_split].to_vec()),
                push_step(wire[second_split..].to_vec()),
                end_step(),
            ],
        )
    })
    .collect()
}

fn decoder_split_fixtures() -> Vec<Fixture> {
    let wire = vec![0, 0, 0, 4, 10, 20, 30, 40];
    (0..=wire.len())
        .map(|split| {
            decoder_fixture(
                format!("decoder split point {split}"),
                None,
                vec![
                    push_step(wire[..split].to_vec()),
                    push_step(wire[split..].to_vec()),
                    end_step(),
                ],
            )
        })
        .collect()
}

fn decoder_failure_fixtures() -> Vec<Fixture> {
    vec![
        decoder_fixture(
            "decoder truncated header",
            None,
            vec![push_step(vec![0, 0, 0]), end_step(), end_step()],
        ),
        decoder_fixture(
            "decoder truncated payload",
            None,
            vec![push_step(vec![0, 0, 0, 2, 1]), end_step(), end_step()],
        ),
        decoder_fixture(
            "decoder oversized terminal failure",
            Some(3),
            vec![push_step(vec![0, 0, 0, 4]), push_step(Vec::new()), end_step()],
        ),
        decoder_fixture(
            "decoder prior frame lost on same-push failure",
            Some(3),
            vec![push_step(vec![0, 0, 0, 1, 1, 0, 0, 0, 4]), push_step(Vec::new())],
        ),
    ]
}

fn decoder_terminal_fixtures() -> Vec<Fixture> {
    vec![
        decoder_fixture(
            "decoder exact configured maximum",
            Some(3),
            vec![push_step(vec![0, 0, 0, 3, 1, 2, 3]), end_step()],
        ),
        decoder_fixture(
            "decoder ended terminal state",
            None,
            vec![end_step(), end_step(), push_step(Vec::new())],
        ),
        decoder_fixture(
            "decoder default maximum",
            None,
            vec![push_step(vec![1, 0, 0, 1]), push_step(Vec::new()), end_step()],
        ),
    ]
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = stateless_fixtures();
    fixtures.extend(decoder_stream_fixtures());
    fixtures.extend(decoder_boundary_fixtures());
    fixtures.extend(decoder_split_fixtures());
    fixtures.extend(decoder_failure_fixtures());
    fixtures.extend(decoder_terminal_fixtures());
    fixtures
}

#[test]
fn framing_matches_the_pinned_typescript_oracle() {
    let mut oracle = TsOracle::spawn();

    for (index, fixture) in fixtures().iter().enumerate() {
        let id = u64::try_from(index + 1).expect("fixture index must fit in request id");
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);
        assert_eq!(actual, expected, "differential mismatch: {}", fixture.name);
    }
}
