use std::ffi::OsString;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::Value;

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));

#[derive(Debug)]
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
    pub(crate) fn spawn(oracle_filename: &str, source_module: &str) -> Self {
        Self::spawn_with_environment(oracle_filename, source_module, &[])
    }

    pub(crate) fn spawn_with_environment(
        oracle_filename: &str,
        source_module: &str,
        env_pairs: &[(&str, OsString)],
    ) -> Self {
        let source_lock = read_source_lock();
        verify_source_head(&source_lock);
        let mut child = spawn_oracle_process(&source_lock, oracle_filename, source_module, env_pairs);
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
        kill_oracle_process_group(&mut self.child);
        let _ = self.child.wait();
    }
}

fn kill_oracle_process_group(child: &mut Child) {
    use rustix::process::{Pid, Signal, kill_process_group};

    if let Some(pid) = Pid::from_raw(child.id() as i32) {
        let _ = kill_process_group(pid, Signal::KILL);
    }
    let _ = child.kill();
}

fn watchdog_node_options(watchdog: &std::path::Path) -> OsString {
    let mut options = std::env::var_os("NODE_OPTIONS").unwrap_or_default();
    if !options.is_empty() {
        options.push(" ");
    }
    options.push("--import ");
    options.push(watchdog);
    options
}

fn read_source_lock() -> SourceLock {
    let value: Value = serde_json::from_str(SOURCE_LOCK).expect("source lock must be valid JSON");
    let worktree = value["worktree"]
        .as_str()
        .map(PathBuf::from)
        .expect("source lock worktree must be a string");
    let commit = value["commit"]
        .as_str()
        .map(str::to_owned)
        .expect("source lock commit must be a string");
    SourceLock { worktree, commit }
}

fn spawn_oracle_process(
    source_lock: &SourceLock,
    oracle_filename: &str,
    source_module: &str,
    env_pairs: &[(&str, OsString)],
) -> Child {
    let tsx = source_lock.worktree.join("node_modules/.bin/tsx");
    let tsconfig = source_lock.worktree.join("tsconfig.json");
    let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle")
        .join(oracle_filename);
    let watchdog = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/parent-watchdog.mjs");

    let mut command = Command::new(&tsx);
    command
        .arg("--tsconfig")
        .arg(tsconfig)
        .arg(oracle)
        .arg(source_lock.worktree.join(source_module))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .process_group(0);
    for (name, value) in env_pairs {
        command.env(name, value);
    }
    command
        .env("PI_ORACLE_PARENT_PID", std::process::id().to_string())
        .env("NODE_OPTIONS", watchdog_node_options(&watchdog));

    command
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
