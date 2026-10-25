#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_transcript::{
    create_session_transcript_read_stream, read_session_transcript_bytes, read_session_transcript_text,
    session_transcript_read_error,
};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use serde_json::{Value, json};
use std::io::Read;

fn native_failure(path: &str, mode: &str) -> Value {
    let result = match mode {
        "bytes" => read_session_transcript_bytes(path).map(|_| ()),
        "text" => read_session_transcript_text(path).map(|_| ()),
        "stream" => create_session_transcript_read_stream(path).and_then(|mut stream| {
            stream
                .read_to_end(&mut Vec::new())
                .map(|_| ())
                .map_err(|error| session_transcript_read_error(&path.into(), error))
        }),
        _ => panic!("unexpected mode"),
    };
    let Err(error) = result else { return json!({"ok":true}) };
    let manager_error = pi_coding_agent::core::session_manager::SessionManagerError::from(error);
    let AbortReason::Error(error) = CallbackError::from(manager_error).thrown_value() else {
        panic!("filesystem throws an Error");
    };
    json!({"ok":false,"error":error.value().to_serde().unwrap(),
        "stringValue":pi_js_compat::js_json::JsValue::String(error.string_value()).to_serde().unwrap(),
        "constructor":error.constructor_class().name()})
}

#[test]
fn transcript_filesystem_errors_keep_frozen_error_identity_and_fields() {
    // Frozen session-transcript.ts:17-22 uses readFileSync; :25-31 uses createReadStream.
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("missing.jsonl").to_str().unwrap().to_owned();
    let not_directory = tmp.path().join("file");
    std::fs::write(&not_directory, b"plain").unwrap();
    let nested = not_directory.join("session.jsonl").to_str().unwrap().to_owned();
    let nul = format!("{missing}\0tail");
    let mut ts = oracle::TsOracle::spawn("storage-transcript-oracle.mts", "packages/coding-agent/src/core");
    let mut mismatches = Vec::new();
    for path in [&missing, &nested, &nul, tmp.path().to_str().unwrap()] {
        for mode in ["bytes", "text", "stream"] {
            let response = ts.request(&json!({"id":1,"path":path,"mode":mode}));
            assert_eq!(response["ok"], true, "{response}");
            let native = native_failure(path, mode);
            if native != response["output"] {
                mismatches.push(format!(
                    "{path:?} {mode}: native {native}, frozen {}",
                    response["output"]
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
