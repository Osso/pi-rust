use crate::shared_oracle as oracle;

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
fn transcript_frames_are_bidirectionally_readable_without_restoration() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = tmp.path().join("frames.jsonl");
    let archive = tmp.path().join("frames.jsonl.zst");
    let payload = "漢😀é\n".repeat(40000).into_bytes();
    let mut ts = oracle::TsOracle::spawn("storage-transcript-oracle.mts", "packages/coding-agent/src/core");
    let written = ts.request(&json!({"id":1,"op":"write","path":archive,"bytes":payload}));
    assert_eq!(written["output"]["written"], true);
    let ts_frame = std::fs::read(&archive).unwrap();
    assert_eq!(read_session_transcript_bytes(plain.to_str().unwrap()).unwrap(), payload);
    assert_eq!(std::fs::read(&archive).unwrap(), ts_frame);
    assert!(!plain.exists());
    let native_frame = zstd::stream::encode_all(&payload[..], 0).unwrap();
    // A real empty skippable frame (0x184d2a50), not an arbitrary garbage tail.
    let skip = [0x50, 0x2a, 0x4d, 0x18, 0, 0, 0, 0];
    for (bytes, expected) in [
        (native_frame.clone(), payload.clone()),
        ([skip.as_slice(), native_frame.as_slice()].concat(), payload.clone()),
        (
            [native_frame.as_slice(), skip.as_slice(), ts_frame.as_slice()].concat(),
            payload.repeat(2),
        ),
        (
            [native_frame.as_slice(), b"non-frame trailing bytes"].concat(),
            payload.clone(),
        ),
    ] {
        std::fs::write(&archive, &bytes).unwrap();
        let read = ts.request(&json!({"id":1,"op":"read","path":plain}));
        assert_eq!(read["ok"], true, "{read}");
        assert_eq!(read["output"]["bytes"], json!(expected));
        assert_eq!(
            read_session_transcript_bytes(plain.to_str().unwrap()).unwrap(),
            expected
        );
        assert_eq!(read["output"]["text"], read["output"]["streamed"]);
        let mut stream = create_session_transcript_read_stream(plain.to_str().unwrap()).unwrap();
        let mut actual = Vec::new();
        let mut chunk = [0; 13];
        loop {
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 {
                break;
            }
            actual.extend_from_slice(&chunk[..count]);
        }
        assert_eq!(actual, expected);
        assert_eq!(std::fs::read(&archive).unwrap(), bytes);
        assert!(!plain.exists());
    }
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
