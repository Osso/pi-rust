//! Process harness shared by the print/RPC mode differential tests.
//!
//! Spawns the real pinned TypeScript mode and the Rust mode harness over
//! stdin/stdout, collects output lines in wire order, and normalizes only
//! ids, timestamps, durations, and temp paths.

#![allow(dead_code)] // Each integration test compiles its own copy and uses a subset.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));

pub(crate) const LINE_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) struct SourceLock {
    pub worktree: PathBuf,
    pub commit: String,
}

pub(crate) fn read_source_lock() -> SourceLock {
    let value: Value = serde_json::from_str(SOURCE_LOCK).expect("source lock must be valid JSON");
    SourceLock {
        worktree: PathBuf::from(value["worktree"].as_str().expect("source lock worktree")),
        commit: value["commit"].as_str().expect("source lock commit").to_owned(),
    }
}

pub(crate) fn verify_source_head(lock: &SourceLock) {
    let output = Command::new("git")
        .arg("-C")
        .arg(&lock.worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git must verify the pinned TypeScript source HEAD");
    assert!(output.status.success(), "git rev-parse failed");
    let actual = String::from_utf8(output.stdout).expect("git HEAD must be UTF-8");
    assert_eq!(
        actual.trim(),
        lock.commit,
        "TypeScript source HEAD must equal docs/porting/source-lock.json"
    );
}

pub(crate) struct ModeHarness {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Option<String>>,
    stderr: Arc<Mutex<String>>,
    pub collected: Vec<String>,
}

impl ModeHarness {
    pub(crate) fn spawn_typescript(lock: &SourceLock, module: &str, env: &[(&str, String)], cwd: &Path) -> Self {
        let tsx = lock.worktree.join("node_modules/.bin/tsx");
        let tsconfig = lock.worktree.join("tsconfig.json");
        let harness = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/modes-harness.ts");
        let mut command = Command::new(tsx);
        command
            .arg("--tsconfig")
            .arg(tsconfig)
            .arg(harness)
            .arg(lock.worktree.join(module))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in env {
            command.env(name, value);
        }
        Self::spawn(command)
    }

    pub(crate) fn spawn_rust(env: &[(&str, String)], cwd: &Path) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pi-mode-harness"));
        command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in env {
            command.env(name, value);
        }
        Self::spawn(command)
    }

    fn spawn(mut command: Command) -> Self {
        let mut child = command.spawn().expect("mode harness must start");
        let stdin = child.stdin.take().expect("stdin must be piped");
        let stdout = child.stdout.take().expect("stdout must be piped");
        let mut stderr = child.stderr.take().expect("stderr must be piped");
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(Some(line)).is_err() {
                    break;
                }
            }
            let _ = sender.send(None);
        });
        let collected_stderr = Arc::new(Mutex::new(String::new()));
        let stderr_sink = Arc::clone(&collected_stderr);
        std::thread::spawn(move || {
            let mut buffer = String::new();
            let _ = std::io::Read::read_to_string(&mut stderr, &mut buffer);
            *stderr_sink.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = buffer;
        });
        Self {
            child,
            stdin: Some(stdin),
            lines,
            stderr: collected_stderr,
            collected: Vec::new(),
        }
    }

    pub(crate) fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin must stay open");
        stdin.write_all(line.as_bytes()).expect("command write");
        stdin.write_all(b"\n").expect("command newline");
        stdin.flush().expect("command flush");
    }

    /// Collects lines until `predicate` matches, returning the matched line.
    pub(crate) fn wait_for(&mut self, predicate: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + LINE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(remaining) {
                Ok(Some(line)) => {
                    let matched = predicate(&line);
                    self.collected.push(line.clone());
                    if matched {
                        return line;
                    }
                }
                Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                    panic!("harness exited before the expected line; stderr: {}", self.stderr());
                }
                Err(RecvTimeoutError::Timeout) => {
                    panic!(
                        "timed out waiting for a line; last lines: {:?}; stderr: {}",
                        &self.collected[self.collected.len().saturating_sub(5)..],
                        self.stderr()
                    );
                }
            }
        }
    }

    pub(crate) fn wait_for_id(&mut self, id: &str) -> String {
        let needle = format!("\"id\":\"{id}\"");
        self.wait_for(|line| line.contains(&needle))
    }

    /// Closes stdin and collects the remaining output until the process exits.
    pub(crate) fn finish(mut self) -> (Vec<String>, i32, String) {
        drop(self.stdin.take());
        while let Ok(Some(line)) = self.lines.recv_timeout(LINE_TIMEOUT) {
            self.collected.push(line);
        }
        let status = self.child.wait().expect("harness must exit");
        std::thread::sleep(Duration::from_millis(50));
        let stderr = self.stderr();
        (self.collected, status.code().unwrap_or(-1), stderr)
    }

    pub(crate) fn stderr(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Substitutions allowed by the slice brief: ids, timestamps, durations, temp paths.
#[derive(Clone)]
pub(crate) struct Normalizer {
    replacements: Vec<(String, &'static str)>,
}

impl Normalizer {
    pub(crate) fn new() -> Self {
        Self {
            replacements: Vec::new(),
        }
    }

    pub(crate) fn path(mut self, path: &str, placeholder: &'static str) -> Self {
        self.replacements.push((path.to_owned(), placeholder));
        self
    }

    pub(crate) fn normalize(&self, line: &str) -> String {
        let mut line = line.to_owned();
        for (path, placeholder) in &self.replacements {
            line = line.replace(path, placeholder);
        }
        line = replace_timestamps(&line);
        line = replace_uuids(&line);
        line = replace_session_file(&line);
        line = replace_export_path(&line);
        line = replace_entry_ids(&line);
        line
    }
}

fn replace_timestamps(line: &str) -> String {
    let timestamp_field = regex::Regex::new(r#""timestamp":("[^"]*"|-?\d+(?:\.\d+)?)"#).expect("timestamp regex");
    let duration_field =
        regex::Regex::new(r#""(startedAt|finishedAt|durationMs)":-?\d+(?:\.\d+)?"#).expect("duration regex");
    let line = timestamp_field
        .replace_all(line, r#""timestamp":"<timestamp>""#)
        .into_owned();
    duration_field.replace_all(&line, r#""$1":<duration>"#).into_owned()
}

fn replace_uuids(line: &str) -> String {
    let uuid = regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").expect("uuid regex");
    uuid.replace_all(line, "<uuid>").into_owned()
}

fn replace_session_file(line: &str) -> String {
    let session_file = regex::Regex::new(r#""sessionFile":"[^"]*""#).expect("session file regex");
    session_file
        .replace_all(line, r#""sessionFile":"<sessionFile>""#)
        .into_owned()
}

fn replace_export_path(line: &str) -> String {
    let path = regex::Regex::new(r#""path":"[^"]*\.html""#).expect("export path regex");
    path.replace_all(line, r#""path":"<htmlPath>""#).into_owned()
}

fn replace_entry_ids(line: &str) -> String {
    let entry = regex::Regex::new(r#""(id|parentId|leafId)":"[0-9a-f]{8}""#).expect("entry id regex");
    entry.replace_all(line, r#""$1":"<entryId>""#).into_owned()
}

/// Creates the padded package directory used by the Rust harness so both sides
/// derive identically sized documentation paths (and therefore token estimates).
pub(crate) fn create_package_dir(length: usize) -> PathBuf {
    let prefix = "/tmp/pi-mode-pkg-";
    assert!(length > prefix.len(), "package dir must be longer than the prefix");
    let directory = PathBuf::from(format!("{prefix}{}", "x".repeat(length - prefix.len())));
    if !directory.join("theme").exists() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("theme");
        copy_directory(&source, &directory.join("theme"));
    }
    directory
}

fn copy_directory(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("theme directory");
    for entry in std::fs::read_dir(source).expect("theme entries") {
        let entry = entry.expect("theme entry");
        std::fs::copy(entry.path(), target.join(entry.file_name())).expect("theme file copy");
    }
}
