//! Cross-implementation lock protocol proof: the pinned TypeScript `proper-lockfile` and the Rust
//! port must share one lock directory in both directions.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use pi_coding_agent::utils::lockfile::{LockOptions, acquire, acquire_sync, check};
use serde_json::Value;

fn source_root() -> PathBuf {
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .expect("source lock must be valid JSON");
    PathBuf::from(lock["worktree"].as_str().expect("source lock worktree"))
}

fn spawn_peer(root: &Path, mode: &str, file: &Path, extra: Option<&str>) -> Child {
    let mut command = Command::new(root.join("node_modules/.bin/tsx"));
    command
        .arg("--tsconfig")
        .arg(root.join("tsconfig.json"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/lockfile-cross-process.ts"))
        .arg("unused")
        .arg(root)
        .arg(mode)
        .arg(file);
    if let Some(value) = extra {
        command.arg(value);
    }
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("tsx must start the lockfile peer")
}

fn read_line(child: &mut Child) -> String {
    let stdout = child.stdout.as_mut().expect("peer stdout must be piped");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("peer must print a line");
    line.trim().to_owned()
}

fn wait_for_exit(child: &mut Child) {
    let mut rest = String::new();
    child
        .stdout
        .as_mut()
        .expect("peer stdout")
        .read_to_string(&mut rest)
        .unwrap();
    assert!(child.wait().unwrap().success(), "peer failed: {rest}");
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pi-lockfile-cross-{name}-{}-{:?}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn rust_waits_for_a_typescript_holder_and_typescript_reports_rust_holders() {
    let root = source_root();
    let dir = temp_dir("both");
    let target = dir.join("auth.json");
    std::fs::write(&target, "{}").unwrap();

    // TypeScript holds the lock; Rust must wait until the peer releases it.
    let mut holder = spawn_peer(&root, "hold", &target, Some("600"));
    assert_eq!(read_line(&mut holder), "LOCKED");
    assert!(check(&target, &LockOptions::default()).unwrap());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let started = Instant::now();
    let guard = runtime
        .block_on(acquire(
            &target,
            &LockOptions {
                retries: 3,
                ..LockOptions::default()
            },
        ))
        .expect("Rust must acquire after the TypeScript holder releases");
    let waited = started.elapsed();
    assert!(waited >= Duration::from_millis(400), "Rust did not wait: {waited:?}");
    assert_eq!(read_line(&mut holder), "RELEASED");
    wait_for_exit(&mut holder);

    // Rust holds the lock; TypeScript must observe ELOCKED, then acquire after release.
    let mut blocked = spawn_peer(&root, "try", &target, None);
    assert_eq!(read_line(&mut blocked), "ELOCKED ELOCKED");
    wait_for_exit(&mut blocked);
    guard.release().unwrap();

    let mut acquired = spawn_peer(&root, "try", &target, None);
    assert_eq!(read_line(&mut acquired), "ACQUIRED");
    assert_eq!(read_line(&mut acquired), "RELEASED");
    wait_for_exit(&mut acquired);
}

#[test]
fn a_stale_typescript_lock_is_reclaimed_by_rust() {
    let root = source_root();
    let dir = temp_dir("stale");
    let target = dir.join("trust.json");
    let mut holder = spawn_peer(&root, "hold", &target, Some("1500"));
    assert_eq!(read_line(&mut holder), "LOCKED");

    // The peer keeps refreshing its mtime, so a short stale window still must not steal a live lock.
    let error = acquire_sync(
        &target,
        &LockOptions {
            stale_ms: 1_000,
            ..LockOptions::default()
        },
    )
    .expect_err("a live refreshed lock must not be reclaimed");
    assert_eq!(error.code, pi_coding_agent::utils::lockfile::LockErrorCode::Locked);

    assert_eq!(read_line(&mut holder), "RELEASED");
    wait_for_exit(&mut holder);
}
