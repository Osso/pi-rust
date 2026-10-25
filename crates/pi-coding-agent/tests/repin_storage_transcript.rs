#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_transcript::{
    create_session_transcript_read_stream, read_session_transcript_bytes, read_session_transcript_text,
    session_transcript_read_error,
};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use serde_json::{Value, json};
use std::io::{Read, Write};

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
fn transcript_codec_errors_keep_frozen_error_identity_and_fields() {
    // Frozen session-transcript.ts:20 invokes Node zstdDecompressSync directly.
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("codec.jsonl.zst");
    let compressed = zstd::stream::encode_all(&b"native fixture bytes"[..], 0).unwrap();
    let mut reserved = compressed.clone();
    reserved[4] |= 0x08;
    let mut window = compressed.clone();
    window[5] = 0xf8;
    let mut dictionary = compressed.clone();
    dictionary[4] |= 1;
    dictionary.insert(6, 1);
    let mut corrupt = compressed.clone();
    corrupt[6] |= 6;
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 0).unwrap();
    encoder.include_checksum(true).unwrap();
    encoder.write_all(b"checksummed native fixture").unwrap();
    let mut checksum = encoder.finish().unwrap();
    *checksum.last_mut().unwrap() ^= 1;
    let mut ts = oracle::TsOracle::spawn("storage-transcript-oracle.mts", "packages/coding-agent/src/core");
    let mut mismatches = Vec::new();
    for (case, bytes) in [
        ("empty", Vec::new()),
        ("bad-magic", vec![0, 1, 2, 3]),
        ("short-magic", vec![0x28, 0xb5]),
        ("truncated", compressed[..compressed.len() - 1].to_vec()),
        ("reserved-header-bit", vec![0x28, 0xb5, 0x2f, 0xfd, 0x08]),
        ("unsupported-header", reserved),
        ("oversized-window", window),
        ("unknown-dictionary", dictionary),
        ("reserved-block", corrupt),
        ("bad-checksum", checksum),
    ] {
        std::fs::write(&path, bytes).unwrap();
        for mode in ["bytes", "text"] {
            let response = ts.request(&json!({"id":1,"path":path,"mode":mode}));
            assert_eq!(response["ok"], true, "{response}");
            let native = native_failure(path.to_str().unwrap(), mode);
            if native != response["output"] {
                mismatches.push(format!("{case} {mode}: native {native}, frozen {}", response["output"]));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
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
