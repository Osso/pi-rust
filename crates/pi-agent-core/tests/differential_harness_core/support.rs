use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::Value;

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));

struct SourceLock {
    worktree: PathBuf,
    commit: String,
}

pub(crate) struct TsOracle {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl TsOracle {
    pub(crate) fn spawn(source_modules: &[&str]) -> Self {
        let source_lock = read_source_lock();
        verify_source_head(&source_lock);
        let mut child = spawn_oracle_process(&source_lock, source_modules);
        let stdin = BufWriter::new(child.stdin.take().expect("oracle stdin must be piped"));
        let stdout = BufReader::new(child.stdout.take().expect("oracle stdout must be piped"));
        Self { child, stdin, stdout }
    }

    pub(crate) fn request(&mut self, request: &Value) -> Value {
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

fn read_source_lock() -> SourceLock {
    let value: Value = serde_json::from_str(SOURCE_LOCK).expect("source lock must be valid JSON");
    SourceLock {
        worktree: value["worktree"]
            .as_str()
            .map(PathBuf::from)
            .expect("source lock worktree must be a string"),
        commit: value["commit"]
            .as_str()
            .map(str::to_owned)
            .expect("source lock commit must be a string"),
    }
}

fn spawn_oracle_process(source_lock: &SourceLock, source_modules: &[&str]) -> Child {
    let tsx = source_lock.worktree.join("node_modules/.bin/tsx");
    let tsconfig = source_lock.worktree.join("tsconfig.json");
    let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle")
        .join("harness-core-oracle.ts");

    let mut command = Command::new(&tsx);
    command.arg("--tsconfig").arg(tsconfig).arg(oracle);
    for module in source_modules {
        command.arg(source_lock.worktree.join(module));
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("worktree-local tsx must start the TypeScript oracle")
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

const SOURCE_MODULES: [&str; 6] = [
    "packages/agent/src/harness/events.ts",
    "packages/agent/src/harness/messages.ts",
    "packages/agent/src/harness/skills.ts",
    "packages/agent/src/harness/prompt-templates.ts",
    "packages/agent/src/harness/env/nodejs.ts",
    "packages/agent/src/harness/reducer.ts",
];

pub(crate) fn spawn_oracle() -> TsOracle {
    TsOracle::spawn(&SOURCE_MODULES)
}

pub(crate) fn expect_ok(response: &Value) -> &Value {
    assert_eq!(response["ok"], serde_json::json!(true), "oracle failed: {response}");
    &response["output"]
}

pub(crate) fn oracle_request(id: u64, operation: &str, payload: Value) -> Value {
    let mut request = payload;
    let object = request.as_object_mut().expect("request payload must be an object");
    object.insert("id".to_owned(), serde_json::json!(id));
    object.insert("operation".to_owned(), serde_json::json!(operation));
    request
}
