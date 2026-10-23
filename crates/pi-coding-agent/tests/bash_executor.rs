//! `core/bash-executor.ts` behavioral tests.

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use pi_coding_agent::core::bash_executor::{BashExecutorOptions, execute_bash_with_operations};
use pi_coding_agent::core::tools::bash::create_local_bash_operations;

fn operations() -> Arc<dyn pi_coding_agent::core::tools::bash::BashOperations> {
    Arc::new(create_local_bash_operations(None))
}

fn cwd() -> JsString {
    std::env::current_dir()
        .expect("current directory")
        .to_string_lossy()
        .into_owned()
        .into()
}

#[tokio::test]
async fn preserves_sanitization_when_using_local_bash_operations() {
    let result = execute_bash_with_operations(
        &"printf '\\033[31mred\\033[0m\\r\\n'".into(),
        &cwd(),
        operations(),
        &BashExecutorOptions::default(),
    )
    .await
    .expect("execute");
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.output, "red\n");
    assert!(!result.cancelled);
    assert!(!result.truncated);
    assert_eq!(result.full_output_path, None);
}

#[tokio::test]
async fn persists_full_output_when_truncation_happens_by_line_count_only() {
    let result = execute_bash_with_operations(
        &"for i in $(seq 1 3000); do printf 'x\\n'; done".into(),
        &cwd(),
        operations(),
        &BashExecutorOptions::default(),
    )
    .await
    .expect("execute");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.truncated);
    let full_output_path = result.full_output_path.expect("full output path");
    let full_output = fs::read_to_string(encode_node_utf8(&full_output_path)).expect("full output");
    assert_eq!(full_output.split('\n').filter(|line| *line == "x").count(), 3000);
    let _ = fs::remove_file(encode_node_utf8(&full_output_path));
}

#[tokio::test]
async fn streams_sanitized_chunks_to_the_callback() {
    let chunks: Arc<Mutex<Vec<JsString>>> = Arc::new(Mutex::new(Vec::new()));
    let collector = Arc::clone(&chunks);
    let result = execute_bash_with_operations(
        &"printf 'one\\n'; printf 'two\\n'".into(),
        &cwd(),
        operations(),
        &BashExecutorOptions {
            on_chunk: Some(Arc::new(move |chunk: &JsString| {
                collector.lock().expect("chunks").push(chunk.to_owned());
            })),
            signal: None,
        },
    )
    .await
    .expect("execute");
    assert_eq!(result.output, "one\ntwo\n");
    assert_eq!(JsString::join(chunks.lock().expect("chunks").iter(), ""), "one\ntwo\n");
}

#[tokio::test]
async fn reports_the_process_exit_code() {
    let result = execute_bash_with_operations(
        &"printf 'failed'; exit 3".into(),
        &cwd(),
        operations(),
        &BashExecutorOptions::default(),
    )
    .await
    .expect("execute");
    assert_eq!(result.exit_code, Some(3));
    assert_eq!(result.output, "failed");
    assert!(!result.cancelled);
}

#[tokio::test]
async fn reports_cancellation_without_an_exit_code() {
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let controller_cancel = controller_signal.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        controller_cancel.abort(None);
    });
    let result = execute_bash_with_operations(
        &"printf 'started\\n'; sleep 5".into(),
        &cwd(),
        operations(),
        &BashExecutorOptions {
            on_chunk: None,
            signal: Some(signal),
        },
    )
    .await
    .expect("execute");
    assert!(result.cancelled);
    assert_eq!(result.exit_code, None);
    assert!(result.output.contains("started"));
}

#[tokio::test]
async fn keeps_output_below_the_truncation_threshold_untouched() {
    let result = execute_bash_with_operations(
        &"printf 'small output'".into(),
        &cwd(),
        operations(),
        &BashExecutorOptions::default(),
    )
    .await
    .expect("execute");
    assert_eq!(result.output, "small output");
    assert!(!result.truncated);
    assert_eq!(result.full_output_path, None);
}
