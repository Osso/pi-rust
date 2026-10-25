use crate::shared_oracle as oracle;

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
            "path" => Ok(json!({"path":plain})),
            "remove" | "replace" => {
                let bytes = std::fs::read(encode_node_utf8(plain)).unwrap();
                let encoded = encode_node_utf8(plain);
                let directory = std::path::Path::new(&encoded).parent().unwrap();
                std::fs::remove_dir_all(directory).unwrap();
                if kind == "replace" {
                    std::fs::write(directory, b"replacement").unwrap();
                }
                Ok(json!({"bytes":bytes}))
            }
            "cleanup" => {
                let directory = std::path::Path::new(temporary.as_ref().unwrap()).parent().unwrap();
                std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o000)).unwrap();
                Err(CallbackError::new(
                    "callback failure must be replaced by cleanup failure",
                ))
            }
            "callback" => Err(CallbackError::from(
                NativeNodeError::from_constructor(NativeErrorClass::TypeError, "callback failed")
                    .with_property("code", JsValue::String("FIXTURE".into())),
            )),
            "null" => Err(CallbackError::from_rejection(AbortReason::Value(JsValue::Null))),
            _ => Ok(json!({"bytes":std::fs::read(encode_node_utf8(plain)).unwrap(),
                "mode":std::fs::metadata(encode_node_utf8(plain)).unwrap().permissions().mode() & 0o777})),
        }
    });
    if kind == "cleanup" {
        let directory = std::path::Path::new(temporary.as_ref().unwrap()).parent().unwrap();
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
        let error = result.unwrap_err().thrown_value().as_error().unwrap().value();
        return json!({"ok":false,"error":error.to_serde().unwrap(),"directory":directory});
    }
    if kind == "replace" {
        let directory = std::path::Path::new(temporary.as_ref().unwrap()).parent().unwrap();
        if directory.exists() {
            std::fs::remove_file(directory).unwrap();
        }
    }
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
fn archived_spool_needs_no_packaged_pi_binary() {
    if let Ok(path) = std::env::var("PI_STORAGE_NO_PACKAGE_ARCHIVE") {
        let mut ts = oracle::TsOracle::spawn("storage-spool-oracle.mts", "packages/coding-agent/src/core");
        let expected = ts.request(&json!({"id":1,"path":path,"kind":"read"}));
        assert_eq!(expected["output"]["ok"], true);
        assert_eq!(native_spool(&path, "read"), expected["output"]);
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("fixture.jsonl.zst");
    std::fs::write(
        &archive,
        zstd::stream::encode_all(&b"SDK archive without package executable"[..], 0).unwrap(),
    )
    .unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "archived_spool_needs_no_packaged_pi_binary", "--nocapture"])
        .env("PI_STORAGE_NO_PACKAGE_ARCHIVE", &archive)
        .env("PI_PACKAGE_DIR", tmp.path().join("no-package"))
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}{}",
        String::from_utf8(child.stdout).unwrap(),
        String::from_utf8(child.stderr).unwrap()
    );
}

#[test]
fn plain_pass_through_and_already_removed_spools_match_frozen() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = tmp.path().join("plain.jsonl");
    std::fs::write(&plain, b"existing plain").unwrap();
    let missing = tmp.path().join("missing.jsonl");
    let other = tmp.path().join("ordinary.txt");
    let mut ts = oracle::TsOracle::spawn("storage-spool-oracle.mts", "packages/coding-agent/src/core");
    for path in [
        plain.to_str().unwrap(),
        missing.to_str().unwrap(),
        other.to_str().unwrap(),
        "",
    ] {
        let expected = ts.request(&json!({"id":1,"path":path,"kind":"path"}));
        assert_eq!(native_spool(path, "path"), expected["output"]);
    }
    let archive = tmp.path().join("fixture.jsonl.zst");
    let bytes = zstd::stream::encode_all(&b"no leftover temporary files"[..], 0).unwrap();
    std::fs::write(&archive, &bytes).unwrap();
    for kind in ["remove", "replace"] {
        let expected = ts.request(&json!({"id":2,"path":archive,"kind":kind}));
        assert_eq!(
            native_spool(archive.to_str().unwrap(), kind),
            expected["output"],
            "{kind}"
        );
    }
    assert_eq!(std::fs::read(archive).unwrap(), bytes);
}

#[test]
fn cleanup_failure_overrides_callback_failure_with_frozen_rm_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("cleanup.jsonl.zst");
    std::fs::write(&path, zstd::stream::encode_all(&b"private plaintext"[..], 0).unwrap()).unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-spool-oracle.mts", "packages/coding-agent/src/core");
    let source = ts.request(&json!({"id":1,"path":path,"kind":"cleanup"}));
    assert_eq!(source["output"]["ok"], false);
    let native = native_spool(path.to_str().unwrap(), "cleanup");
    let normalize = |mut value: Value| {
        let directory = value["directory"].as_str().unwrap().to_owned();
        assert_eq!(
            value["error"]["path"], directory,
            "the failure path must be exactly its own temp directory"
        );
        let message = value["error"]["message"].as_str().unwrap();
        assert!(message.ends_with(&format!("'{directory}'")));
        value["error"]["message"] = json!(message.replace(&directory, "<temporary>"));
        value["error"]["path"] = json!("<temporary>");
        value["directory"] = json!("<temporary>");
        value
    };
    assert_eq!(normalize(native), normalize(source["output"].clone()));
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

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
