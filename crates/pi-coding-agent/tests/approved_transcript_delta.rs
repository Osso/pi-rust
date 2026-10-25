use crate::shared_oracle as oracle;

use pi_coding_agent::core::session_transcript::create_session_transcript_read_stream;
use serde_json::{Value, json};
use std::io::Read;

fn approved_frozen_stderr() -> String {
    let record: Value =
        serde_json::from_str(include_str!("../../../docs/porting/deltas/transcript-stream.json")).unwrap();
    record["frozen"]["normalizedStderr"].as_str().unwrap().to_owned()
}

// Only replace numeric locations in the node:events source header and Node-internal
// stack frames. Preserve function/module names, punctuation (including ` {`), all
// other lines, whitespace, and the final newline. No paths or versions are erased.
fn normalize_frozen_stderr(stderr: &str) -> String {
    let header = regex::Regex::new(r"(?m)^node:events:[0-9]+$").unwrap();
    let frames = regex::Regex::new(r"(?m)^(    at [^\n]*\(node:[^()\n]+):[0-9]+:[0-9]+(\)(?: \{)?)$").unwrap();
    let normalized_header = header.replace_all(stderr, "node:events:<line>");
    frames
        .replace_all(&normalized_header, "${1}:<line>:<column>${2}")
        .into_owned()
}

#[test]
fn stderr_normalizer_preserves_unrelated_errors() {
    let stderr = r"node:events:505
    throw er; // Unhandled 'error' event
    ^

Error: unexpected end of file
    at genericNodeError (node:internal/errors:999:15)
    at wrappedFn (node:internal/errors:544:14)
    at ZstdDecompress.zlibOnError [as onerror] (node:zlib:210:13)
Emitted 'error' event on ZstdDecompress instance at:
    at emitErrorNT (node:internal/streams/destroy:170:8)
    at emitErrorCloseNT (node:internal/streams/destroy:129:3)
    at process.processTicksAndRejections (node:internal/process/task_queues:90:21) {
  errno: -5,
  code: 'Z_BUF_ERROR'
}

Node.js v26.11.1
";
    let approved = approved_frozen_stderr();
    assert_eq!(normalize_frozen_stderr(stderr), approved);
    let appended = format!("{stderr}Error: unrelated additional failure\n");
    let normalized_appended = normalize_frozen_stderr(&appended);
    assert_ne!(normalized_appended, approved);
    assert_eq!(
        normalized_appended,
        format!("{approved}Error: unrelated additional failure\n")
    );
    let different_error = stderr.replace("unexpected end of file", "different failure");
    assert_ne!(normalize_frozen_stderr(&different_error), approved);
}

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
    assert_eq!(normalize_frozen_stderr(stderr), approved_frozen_stderr(), "{response}");
    assert_native_stream_error(&path);
}
