use std::path::Path;
use std::time::{Duration, Instant};

use pi_coding_agent::utils::child_process::{
    ChildStdio, ChildStdioOptions, SpawnSyncOptions, spawn_process, spawn_sync, wait_for_child_process,
};

#[tokio::test]
async fn captures_output_and_exit_code() {
    let mut child = spawn_process(
        "sh",
        &["-c", "printf out; printf err >&2; exit 3"],
        ChildStdioOptions::CAPTURE,
    )
    .expect("spawn");
    let output = wait_for_child_process(&mut child).await.expect("wait");

    assert_eq!(output.status, Some(3));
    assert_eq!(output.stdout, b"out");
    assert_eq!(output.stderr, b"err");
}

#[tokio::test]
async fn captures_output_written_after_exit_by_a_descendant() {
    let mut child = spawn_process(
        "sh",
        &[
            "-c",
            "printf HEAD; (for i in 1 2 3 4 5 6; do sleep 0.05; printf TICK$i; done) &",
        ],
        ChildStdioOptions::CAPTURE,
    )
    .expect("spawn");
    let output = wait_for_child_process(&mut child).await.expect("wait");

    assert_eq!(output.status, Some(0));
    assert_eq!(
        output.stdout, b"HEADTICK1TICK2TICK3TICK4TICK5TICK6",
        "stderr: {:?}",
        output.stderr
    );
}

#[tokio::test]
async fn releases_a_quiet_descendant_after_the_grace_period() {
    let mut child = spawn_process("sh", &["-c", "(sleep 2) &"], ChildStdioOptions::CAPTURE).expect("spawn");
    let started = Instant::now();
    let output = wait_for_child_process(&mut child).await.expect("wait");

    assert_eq!(output.status, Some(0));
    assert!(output.stdout.is_empty());
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
}

#[tokio::test]
async fn reports_a_signal_termination_without_an_exit_code() {
    let mut child = spawn_process("sh", &["-c", "kill -TERM $$"], ChildStdioOptions::CAPTURE).expect("spawn");
    let output = wait_for_child_process(&mut child).await.expect("wait");
    assert_eq!(output.status, None);
}

#[tokio::test]
async fn spawn_failures_are_reported() {
    assert!(spawn_process("pi-definitely-missing-command", &[], ChildStdioOptions::CAPTURE).is_err());
}

#[test]
fn sync_spawn_captures_output_status_and_cwd() {
    let output = spawn_sync(
        &"sh".into(),
        &["-c".into(), "printf out; printf err >&2; exit 2".into()],
        Default::default(),
    )
    .expect("spawn");
    assert_eq!(output.status, Some(2));
    assert_eq!(output.stdout.as_deref(), Some(b"out".as_slice()));
    assert_eq!(output.stderr.as_deref(), Some(b"err".as_slice()));
    assert!(output.error.is_none());

    let cwd = Path::new("/tmp");
    let output = spawn_sync(
        &"pwd".into(),
        &[],
        SpawnSyncOptions {
            cwd: Some(cwd),
            ..Default::default()
        },
    )
    .expect("spawn");
    assert!(output.error.is_none());
    assert_eq!(
        String::from_utf8_lossy(output.stdout.as_deref().unwrap()).trim(),
        "/tmp"
    );
}

#[tokio::test]
async fn explicit_stdio_async_pipe_delivers_input_and_ignore_drops_output() {
    use tokio::io::AsyncWriteExt;
    let mut child = spawn_process(
        "/usr/bin/python3",
        &["-c", "import sys; data=sys.stdin.buffer.read(); assert data==b'input'+bytes([0])+b'bytes'; print('ignored'); print('ignored',file=sys.stderr)"],
        ChildStdioOptions { stdin: ChildStdio::Pipe, stdout: ChildStdio::Ignore, stderr: ChildStdio::Ignore },
    ).unwrap();
    assert!(child.stdout.is_none());
    assert!(child.stderr.is_none());
    let mut stdin = child.stdin.take().expect("explicit pipe supplies stdin");
    stdin.write_all(b"input\x00bytes").await.unwrap();
    drop(stdin);
    assert!(child.wait().await.unwrap().success());
}

#[test]
fn explicit_stdio_sync_input_and_null_output_preserve_bytes() {
    let output = spawn_sync(
        &"/usr/bin/python3".into(),
        &[
            "-c".into(),
            "import sys; sys.stdout.buffer.write(sys.stdin.buffer.read()); sys.stderr.write('ignored')".into(),
        ],
        SpawnSyncOptions {
            input: Some(b"input\x00bytes"),
            stdio: ChildStdioOptions {
                stdin: ChildStdio::Pipe,
                stdout: ChildStdio::Null,
                stderr: ChildStdio::Ignore,
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert!(output.error.is_none());
    assert_eq!(output.status, Some(0));
    assert_eq!(output.stdout.as_deref(), Some(b"input\x00bytes".as_slice()));
    assert!(output.stderr.is_none());
}

#[test]
fn explicit_stdio_sync_ignored_output_is_not_captured() {
    let output = spawn_sync(
        &"/usr/bin/python3".into(),
        &[
            "-c".into(),
            "import sys; print('ignored'); print('ignored',file=sys.stderr)".into(),
        ],
        SpawnSyncOptions {
            stdio: ChildStdioOptions {
                stdin: ChildStdio::Ignore,
                stdout: ChildStdio::Ignore,
                stderr: ChildStdio::Ignore,
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(output.status, Some(0));
    assert!(output.stdout.is_none());
    assert!(output.stderr.is_none());
}

#[test]
fn explicit_stdio_sync_timeout_includes_blocked_input_write() {
    let input = vec![b'x'; 4 * 1024 * 1024];
    let started = Instant::now();
    let output = spawn_sync(
        &"/usr/bin/python3".into(),
        &["-c".into(), "import time; time.sleep(5)".into()],
        SpawnSyncOptions {
            input: Some(&input),
            timeout: Some(Duration::from_millis(100)),
            stdio: ChildStdioOptions {
                stdin: ChildStdio::Pipe,
                stdout: ChildStdio::Ignore,
                stderr: ChildStdio::Ignore,
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "blocked write escaped whole-call timeout"
    );
    assert_eq!(
        output.error.unwrap().value().get("code").unwrap(),
        &pi_js_compat::js_json::JsValue::String("ETIMEDOUT".into())
    );
    assert_eq!(output.signal.as_deref(), Some("SIGTERM"));
    assert!(output.stdout.is_none());
    assert!(output.stderr.is_none());
}
