#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/transcript_worker.rs"]
mod transcript_worker;

use pi_coding_agent::core::session_transcript_spool::with_seekable_session_transcript;
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;

fn native_spool(path: &str, kind: &str) -> Value {
    let mut temporary = None;
    let result = with_seekable_session_transcript(&JsString::from(path), |plain| -> Result<Value, CallbackError> {
        temporary = Some(encode_node_utf8(plain));
        match kind {
            "callback" => Err(CallbackError::from(
                NativeNodeError::from_constructor(NativeErrorClass::TypeError, "callback failed")
                    .with_property("code", JsValue::String("FIXTURE".into())),
            )),
            "null" => Err(CallbackError::from_rejection(AbortReason::Value(JsValue::Null))),
            _ => Ok(json!({"bytes":std::fs::read(encode_node_utf8(plain)).unwrap(),
                "mode":std::fs::metadata(encode_node_utf8(plain)).unwrap().permissions().mode() & 0o777})),
        }
    });
    let exists = temporary.is_some_and(|path| std::path::Path::new(&path).exists());
    match result {
        Ok(output) => json!({"ok":true,"output":output,"temporaryExists":exists}),
        Err(error) => {
            let error = match error.thrown_value() {
                AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
                AbortReason::Value(value) => value,
            };
            json!({"ok":false,"error":error.to_serde().unwrap(),"temporaryExists":exists})
        }
    }
}

#[test]
fn spool_callback_and_worker_errors_match_frozen_cleanup_and_error_values() {
    // Frozen session-transcript-spool.ts:49-51 stringifies worker errors;
    // :60-70 preserves callback throws across recursive temporary cleanup.
    transcript_worker::configure_worker();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("fixture.jsonl.zst");
    let valid = zstd::stream::encode_all(&b"source-compatible spool fixture"[..], 0).unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-spool-oracle.mts", "packages/coding-agent/src/core");
    let mut mismatches = Vec::new();
    for (bytes, kind) in [
        (valid.clone(), "read"),
        (valid.clone(), "callback"),
        (valid.clone(), "null"),
        (vec![0, 1, 2, 3], "read"),
        (valid[..valid.len() - 1].to_vec(), "read"),
    ] {
        std::fs::write(&path, bytes).unwrap();
        let expected = ts.request(&json!({"id":1,"path":path,"kind":kind}));
        assert_eq!(expected["ok"], true);
        let actual = native_spool(path.to_str().unwrap(), kind);
        if actual != expected["output"] {
            mismatches.push(format!("{kind}: {actual} != {}", expected["output"]));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
