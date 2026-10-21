#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_transcript::create_session_transcript_read_stream;
use serde_json::{Value, json};
use std::io::Read;

fn source_stream(delta: bool) -> (tempfile::TempDir, String, Value) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("truncated.jsonl.zst").display().to_string();
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "approved-transcript-oracle.mjs",
        "packages/coding-agent/src/core/session-transcript.ts",
        &[("PI_APPROVED_DELTA", if delta { "1" } else { "0" }.into())],
    );
    let response = ts.request(&json!({"path":path}));
    (temp, path, response)
}

fn assert_native_stream_error(path: &str) {
    let mut stream = create_session_transcript_read_stream(path).unwrap();
    let mut decoded = Vec::new();
    let result = stream.read_to_end(&mut decoded);
    assert!(result.is_err(), "truncated public stream must return Err, not success");
    assert!(
        decoded.len() > 1_000_000,
        "exercise late failure after a large decoded payload"
    );
}

#[test]
fn large_truncated_public_stream_error_is_catchable_like_delta() {
    let (_temp, path, response) = source_stream(true);
    assert_eq!(response["status"], 0, "{response}");
    assert_eq!(response["signal"], Value::Null);
    assert_eq!(response["stderr"], "");
    let caught: Value = serde_json::from_str(response["stdout"].as_str().unwrap()).unwrap();
    assert_eq!(caught, json!({"error":"Z_BUF_ERROR"}));
    assert_native_stream_error(&path);
}

#[test]
fn frozen_large_truncation_has_exact_approved_uncaught_error_discrepancy() {
    let (_temp, path, response) = source_stream(false);
    assert_eq!(response["status"], 1, "{response}");
    assert_eq!(response["signal"], Value::Null);
    assert_eq!(response["stdout"], "");
    let stderr = response["stderr"].as_str().unwrap();
    assert!(stderr.contains("Unhandled 'error' event"), "{response}");
    assert!(stderr.contains("Error: unexpected end of file"), "{response}");
    assert!(stderr.contains("code: 'Z_BUF_ERROR'"), "{response}");
    assert_native_stream_error(&path);
}
