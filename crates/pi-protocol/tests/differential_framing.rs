use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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
}

struct Fixture {
    name: &'static str,
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
            } => {
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
        }
    }

    fn rust_response(&self, id: u64) -> Value {
        match &self.operation {
            Operation::Encode(payload) => encode_response(id, payload),
            Operation::Assert {
                frame,
                max_frame_length,
            } => assert_response(id, frame, *max_frame_length),
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

fn encode_fixture(name: &'static str, payload: Vec<u8>) -> Fixture {
    Fixture {
        name,
        operation: Operation::Encode(payload),
    }
}

fn assert_fixture(name: &'static str, frame: Vec<u8>, max_frame_length: Option<u32>) -> Fixture {
    Fixture {
        name,
        operation: Operation::Assert {
            frame,
            max_frame_length,
        },
    }
}

fn repeated_bytes(byte: u8, count: usize) -> Vec<u8> {
    vec![byte; count]
}

fn fixtures() -> Vec<Fixture> {
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
