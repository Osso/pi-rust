use std::path::Path;
use std::time::{Duration, Instant};

use pi_coding_agent::utils::child_process::{spawn_process, spawn_process_sync, wait_for_child_process};

#[tokio::test]
async fn captures_output_and_exit_code() {
    let mut child = spawn_process("sh", &["-c", "printf out; printf err >&2; exit 3"]).expect("spawn");
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
    )
    .expect("spawn");
    let output = wait_for_child_process(&mut child).await.expect("wait");

    assert_eq!(output.status, Some(0));
    assert_eq!(output.stdout, b"HEADTICK1TICK2TICK3TICK4TICK5TICK6");
}

#[tokio::test]
async fn releases_a_quiet_descendant_after_the_grace_period() {
    let mut child = spawn_process("sh", &["-c", "(sleep 2) &"]).expect("spawn");
    let started = Instant::now();
    let output = wait_for_child_process(&mut child).await.expect("wait");

    assert_eq!(output.status, Some(0));
    assert!(output.stdout.is_empty());
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
}

#[tokio::test]
async fn reports_a_signal_termination_without_an_exit_code() {
    let mut child = spawn_process("sh", &["-c", "kill -TERM $$"]).expect("spawn");
    let output = wait_for_child_process(&mut child).await.expect("wait");
    assert_eq!(output.status, None);
}

#[tokio::test]
async fn spawn_failures_are_reported() {
    assert!(spawn_process("pi-definitely-missing-command", &[]).is_err());
}

#[test]
fn sync_spawn_captures_output_status_and_cwd() {
    let output = spawn_process_sync("sh", &["-c", "printf out; printf err >&2; exit 2"], None).expect("spawn");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"out");
    assert_eq!(output.stderr, b"err");

    let cwd = Path::new("/tmp");
    let output = spawn_process_sync("pwd", &[], Some(cwd)).expect("spawn");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "/tmp");
}
