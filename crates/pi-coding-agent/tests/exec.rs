use std::time::{Duration, Instant};

use pi_coding_agent::core::exec::{ExecOptions, exec_command};
use tokio_util::sync::CancellationToken;

fn cwd() -> pi_js_compat::js_json::JsString {
    std::env::temp_dir().to_string_lossy().as_ref().into()
}

#[tokio::test]
async fn captures_stdout_stderr_and_exit_code() {
    let result = exec_command(
        "sh",
        &["-c", "printf out; printf err 1>&2; exit 3"],
        &cwd(),
        &ExecOptions::default(),
    )
    .await;
    assert_eq!(result.stdout, "out");
    assert_eq!(result.stderr, "err");
    assert_eq!(result.code, 3);
    assert!(!result.killed);
}

#[tokio::test]
async fn runs_in_the_requested_directory() {
    let directory = tempfile::tempdir().unwrap();
    let result = exec_command(
        "sh",
        &["-c", "pwd"],
        &directory.path().to_string_lossy(),
        &ExecOptions::default(),
    )
    .await;
    assert_eq!(result.stdout.trim(), directory.path().to_string_lossy());
    assert_eq!(result.code, 0);
}

#[tokio::test]
async fn reports_spawn_failures_as_code_one() {
    let result = exec_command(
        "pi-rust-definitely-missing-binary",
        &[],
        &cwd(),
        &ExecOptions::default(),
    )
    .await;
    assert_eq!(result.code, 1);
    assert!(result.stdout.is_empty());
    assert!(!result.killed);
}

#[tokio::test]
async fn times_out_and_terminates_the_process() {
    let started = Instant::now();
    let result = exec_command(
        "sh",
        &["-c", "sleep 30"],
        &cwd(),
        &ExecOptions {
            timeout: Some(50),
            ..ExecOptions::default()
        },
    )
    .await;
    assert!(result.killed);
    assert_eq!(result.code, 0, "a signal exit maps to code 0");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the command must not run to completion"
    );
}

#[tokio::test]
async fn terminates_immediately_when_the_signal_is_already_aborted() {
    let signal = CancellationToken::new();
    signal.cancel();
    let started = Instant::now();
    let result = exec_command(
        "sh",
        &["-c", "sleep 30"],
        &cwd(),
        &ExecOptions {
            signal: Some(signal),
            ..ExecOptions::default()
        },
    )
    .await;
    assert!(result.killed);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn terminates_when_the_signal_aborts_during_the_run() {
    let signal = CancellationToken::new();
    let canceller = signal.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        canceller.cancel();
    });
    let result = exec_command(
        "sh",
        &["-c", "sleep 30"],
        &cwd(),
        &ExecOptions {
            signal: Some(signal),
            ..ExecOptions::default()
        },
    )
    .await;
    assert!(result.killed);
    assert_eq!(result.code, 0);
}

#[tokio::test]
async fn ignores_non_positive_timeouts() {
    let result = exec_command(
        "sh",
        &["-c", "printf done"],
        &cwd(),
        &ExecOptions {
            timeout: Some(0),
            ..ExecOptions::default()
        },
    )
    .await;
    assert_eq!(result.stdout, "done");
    assert!(!result.killed);
}

#[tokio::test]
async fn decodes_invalid_utf8_lossily() {
    let result = exec_command("sh", &["-c", "printf 'a\\377b'"], &cwd(), &ExecOptions::default()).await;
    assert_eq!(result.stdout, "a\u{fffd}b");
}
