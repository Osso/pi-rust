use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const SOURCE_LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/porting/source-lock.json"
));

struct SourceLock {
    worktree: PathBuf,
    commit: String,
}

fn read_source_lock() -> SourceLock {
    let value: Value = serde_json::from_str(SOURCE_LOCK).expect("source lock must be valid JSON");
    SourceLock {
        worktree: PathBuf::from(value["worktree"].as_str().expect("worktree path")),
        commit: value["commit"].as_str().expect("commit").to_owned(),
    }
}

fn verify_source_head(source_lock: &SourceLock) {
    let output = Command::new("git")
        .arg("-C")
        .arg(&source_lock.worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git must verify the pinned TypeScript source HEAD");
    assert!(output.status.success(), "git rev-parse failed");
    let actual = String::from_utf8(output.stdout).expect("git HEAD must be UTF-8");
    assert_eq!(actual.trim(), source_lock.commit);
}

pub fn source_worktree() -> PathBuf {
    read_source_lock().worktree
}

/// Runs one TypeScript script from `oracle/` with the pinned toolchain.
pub fn run_oracle_script(oracle_filename: &str, arguments: &[std::ffi::OsString]) -> Output {
    let source_lock = read_source_lock();
    verify_source_head(&source_lock);
    let tsx = source_lock.worktree.join("node_modules/.bin/tsx");
    let tsconfig = source_lock.worktree.join("tsconfig.json");
    let oracle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle")
        .join(oracle_filename);
    Command::new("timeout")
        .arg("60")
        .arg(&tsx)
        .arg("--tsconfig")
        .arg(tsconfig)
        .arg(oracle)
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("worktree-local tsx must run the TypeScript oracle script")
}
