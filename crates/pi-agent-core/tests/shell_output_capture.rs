//! Behavioral coverage for the `executeShellWithCapture` port.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::ExecutionEnv;
use pi_agent_core::harness::utils::{ShellCaptureOptions, execute_shell_with_capture};
use tempfile::TempDir;

fn env() -> (TempDir, Arc<dyn ExecutionEnv>) {
    let root = tempfile::tempdir().expect("temp dir");
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: JsString::from(root.path().to_str().expect("utf-8 test path").to_owned()),
        shell_path: None,
        shell_env: Default::default(),
    });
    (root, Arc::new(env))
}

#[tokio::test]
async fn captures_large_output_to_a_full_output_file() {
    let (root, env) = env();
    let result = execute_shell_with_capture(
        &env,
        &JsString::from("yes line | head -n 15000"),
        ShellCaptureOptions::default(),
    )
    .await
    .unwrap();
    assert!(result.truncated);
    let full_output_path = result.full_output_path.clone().expect("full output path");
    let full_output = env.read_text_file(&full_output_path, None).await.unwrap();
    assert!(full_output.split('\n').count() > 10_000);
    assert!(result.output.len() < full_output.len());
    assert!(root.path().exists());
}

#[tokio::test]
async fn captures_a_single_oversized_chunk_to_a_full_output_file() {
    let (_root, env) = env();
    let result = execute_shell_with_capture(
        &env,
        &JsString::from("node -e \"process.stdout.write('start' + 'x'.repeat(128 * 1024) + 'end')\""),
        ShellCaptureOptions::default(),
    )
    .await
    .unwrap();
    assert!(result.truncated);
    let full_output_path = result.full_output_path.clone().expect("full output path");
    let full_output = env.read_text_file(&full_output_path, None).await.unwrap();
    assert!(full_output.contains("start"));
    assert!(full_output.contains("end"));
    assert!(result.output.len() < full_output.len());
    assert!(result.output.ends_with("end"));
}

#[tokio::test]
async fn raw_bytes_trigger_full_output_before_sanitization() {
    let (_root, env) = env();
    let result = execute_shell_with_capture(
        &env,
        &JsString::from("node -e \"process.stdout.write('\\x01'.repeat(11000))\""),
        ShellCaptureOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.output, "");
    assert!(result.truncated);
    let path = result.full_output_path.expect("raw bytes exceed frozen threshold");
    assert_eq!(env.read_text_file(&path, None).await.unwrap(), "");
}

#[tokio::test]
async fn reports_cancelled_commands() {
    let sleep_command = JsString::from("sleep 5");
    let (_root, env) = env();
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let execution = execute_shell_with_capture(
        &env,
        &sleep_command,
        ShellCaptureOptions {
            abort: Some(signal),
            ..ShellCaptureOptions::default()
        },
    );
    controller_signal.abort(None);
    let result = execution.await.unwrap();
    assert!(result.cancelled);
    assert_eq!(result.exit_code, None);
}

#[tokio::test]
async fn returns_execution_errors() {
    let sleep_command = JsString::from("sleep 5");
    let (_root, env) = env();
    let error = execute_shell_with_capture(
        &env,
        &sleep_command,
        ShellCaptureOptions {
            timeout: Some(0.01),
            ..ShellCaptureOptions::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, pi_agent_core::harness::types::ExecutionErrorCode::Timeout);
}

#[tokio::test]
async fn sanitizes_chunks_and_removes_carriage_returns() {
    let (_root, env) = env();
    let result = execute_shell_with_capture(
        &env,
        &JsString::from("printf 'a\\rb\\001c'"),
        ShellCaptureOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(result.output, "abc");
    assert_eq!(result.exit_code, Some(0));
}

#[tokio::test]
async fn reports_sanitized_text_to_chunk_callbacks() {
    let (_root, env) = env();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let result = execute_shell_with_capture(
        &env,
        &JsString::from("printf 'one\\ntwo\\n'"),
        ShellCaptureOptions {
            on_chunk: Some(Arc::new(move |chunk: &JsString| {
                observed.fetch_add(1, Ordering::SeqCst);
                assert!(!chunk.is_empty());
                Ok(())
            })),
            ..ShellCaptureOptions::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(result.output, "one\ntwo\n");
    assert!(calls.load(Ordering::SeqCst) >= 1);
}
