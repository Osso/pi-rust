use crate::shared_oracle as oracle;

use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};

#[tokio::test(flavor = "current_thread")]
async fn public_lifecycle_snapshots_and_original_disk_bytes_match_frozen() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn(
        "output-accumulator-sequence-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    let expected = oracle.request(&json!({"id": 27010, "cases": cases}));
    assert_eq!(expected["ok"], true, "{expected}");
    let root = tempfile::tempdir().unwrap();
    let actual: Vec<_> = cases.iter().map(|case| observe(case, root.path())).collect();
    assert_eq!(JsValue::from(json!(actual)), JsValue::from(expected["output"].clone()));
}

fn observe(case: &Value, root: &std::path::Path) -> Value {
    let path = root.join(case["name"].as_str().unwrap());
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_lines: case["maxLines"].as_f64().unwrap(),
            max_bytes: case["maxBytes"].as_f64().unwrap(),
            ..Default::default()
        },
        path.to_str().unwrap(),
    );
    let mut first_path: Option<JsString> = None;
    let mut frames = Vec::new();
    for action in case["actions"].as_array().unwrap() {
        let frame = match action["type"].as_str().unwrap() {
            "append" => append_frame(&mut output, action),
            "finish" => {
                output.finish().unwrap();
                json!({"type": "finish", "lastLineBytes": output.last_line_bytes()})
            }
            "snapshot" => {
                let snapshot = output
                    .snapshot(SnapshotOptions {
                        persist_if_truncated: action["persist"].as_bool().unwrap(),
                    })
                    .unwrap();
                if first_path.is_none() {
                    first_path.clone_from(&snapshot.full_output_path);
                }
                let bytes = pi_js_compat::js_json::stringify_lossless(
                    &pi_js_compat::js_json::to_value(&snapshot.truncation).unwrap(),
                );
                json!({"type": "snapshot", "content": snapshot.content, "truncation": snapshot.truncation,
                    "truncationJson": bytes, "hasPath": snapshot.full_output_path.is_some(),
                    "firstPathRetained": snapshot.full_output_path == first_path})
            }
            "close" => {
                output.close_temp_file().unwrap();
                let snapshot = output.snapshot(SnapshotOptions::default()).unwrap();
                if first_path.is_none() {
                    first_path.clone_from(&snapshot.full_output_path);
                }
                let hex = first_path.as_ref().map(|_| {
                    std::fs::read(&path)
                        .unwrap()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                });
                json!({"type": "close", "diskHex": hex})
            }
            other => panic!("unknown lifecycle action {other}"),
        };
        frames.push(frame);
    }
    output.close_temp_file().unwrap();
    json!({"name": case["name"], "frames": frames})
}

fn append_frame(output: &mut OutputAccumulator, action: &Value) -> Value {
    let bytes: Vec<u8> = action["bytes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| u8::try_from(value.as_u64().unwrap()).unwrap())
        .collect();
    let error = output.append(&bytes).err().map(|error| error.message);
    json!({"type": "append", "error": error, "lastLineBytes": output.last_line_bytes()})
}

fn cases() -> Vec<Value> {
    let append = |bytes: &[u8]| json!({"type": "append", "bytes": bytes});
    let snapshot = |persist: bool| json!({"type": "snapshot", "persist": persist});
    let finish = json!({"type": "finish"});
    let close = json!({"type": "close"});
    vec![
        json!({"name": "close-before-spill", "maxLines": 100, "maxBytes": 2, "actions": [
            close, snapshot(false), append(b"abc"), snapshot(false), close]}),
        json!({"name": "flush-only-spill", "maxLines": 100, "maxBytes": 2, "actions": [
            append(&[0xe2, 0x82]), snapshot(false), finish, snapshot(false), close, finish, snapshot(true), append(b"late"), close]}),
        json!({"name": "bom-raw-only", "maxLines": 100, "maxBytes": 2, "actions": [
            append(&[0xef, 0xbb, 0xbf]), snapshot(false), finish, snapshot(true), close]}),
        json!({"name": "append-after-close", "maxLines": 100, "maxBytes": 2, "actions": [
            append(b"abc"), snapshot(false), close, append(b"xyz\nlast"), snapshot(true), close, finish, snapshot(true), close]}),
        json!({"name": "negative-untouched", "maxLines": -1, "maxBytes": -1, "actions": [
            snapshot(false), snapshot(true), close, snapshot(true), finish, close]}),
        json!({"name": "empty-append-negative", "maxLines": 100, "maxBytes": -1, "actions": [
            append(b""), snapshot(false), close]}),
        json!({"name": "partial-line-rolling", "maxLines": 2, "maxBytes": 4, "actions": [
            append(b"abcdefghijkl\nfirst\n"), snapshot(false), append(b"second\n"), snapshot(false), finish, snapshot(true), close]}),
    ]
}
