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
fn shared_mkdtemp_keeps_frozen_template_errors_and_suffix_format() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-spool-oracle.mts", "packages/coding-agent/src/core");
    let prefix = tmp.path().join("prefix-").to_str().unwrap().to_owned();
    let expected = ts.request(&json!({"id":1,"op":"mkdtemp","path":prefix,"kind":"read"}));
    assert_eq!(expected["output"]["ok"], true);
    let native = pi_js_compat::node_fs::mkdtemp(&prefix.clone().into()).unwrap();
    for path in [expected["output"]["path"].as_str().unwrap(), &encode_node_utf8(&native)] {
        assert!(path.starts_with(&prefix));
        let suffix = &path[prefix.len()..];
        assert_eq!(suffix.len(), 6);
        assert!(suffix.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o700);
        std::fs::remove_dir(path).unwrap();
    }
    for prefix in [
        format!("{prefix}\0invalid"),
        tmp.path().join("missing/prefix-").to_str().unwrap().to_owned(),
    ] {
        let expected = ts.request(&json!({"id":2,"op":"mkdtemp","path":prefix,"kind":"read"}));
        assert_eq!(expected["output"]["ok"], false);
        let native = pi_js_compat::node_fs::mkdtemp(&prefix.into()).unwrap_err();
        assert_eq!(native.value().to_serde().unwrap(), expected["output"]["error"]);
    }
}

#[test]
fn spool_mkdtemp_failures_preserve_frozen_template_path_and_errno() {
    if let Some(path) = std::env::var_os("PI_STORAGE_TMP_PATH") {
        let real_tmpdir = std::env::var_os("PI_STORAGE_REAL_TMPDIR").unwrap();
        let mut ts = oracle::TsOracle::spawn_with_environment(
            "storage-spool-oracle.mts",
            "packages/coding-agent/src/core",
            &[("TMPDIR", real_tmpdir)],
        );
        let path = path.to_str().unwrap();
        let expected = ts.request(&json!({"id":1,"path":path,"kind":"read","tmpdir":std::env::var("TMPDIR").unwrap()}));
        assert_eq!(expected["output"]["ok"], false);
        assert_eq!(native_spool(path, "read"), expected["output"]);
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("fixture.jsonl.zst");
    std::fs::write(&archive, zstd::stream::encode_all(&b"private fixture"[..], 0).unwrap()).unwrap();
    let regular = tmp.path().join("regular");
    std::fs::write(&regular, b"not a directory").unwrap();
    for root in [tmp.path().join("missing"), regular] {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "spool_mkdtemp_failures_preserve_frozen_template_path_and_errno",
                "--nocapture",
            ])
            .env("PI_STORAGE_TMP_PATH", &archive)
            .env("PI_STORAGE_REAL_TMPDIR", tmp.path())
            .env("TMPDIR", &root)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
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
