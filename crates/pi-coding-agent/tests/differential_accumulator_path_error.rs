use crate::shared_oracle as oracle;

use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_js_compat::js_json::JsValue;
use pi_js_compat::node_error::NativeNodeError;
use serde_json::{Value, json};

#[tokio::test(flavor = "current_thread")]
async fn nul_path_throws_synchronously_and_retains_the_first_path_without_a_stream() {
    assert_first_spill_path_error("append").await;
}

#[tokio::test(flavor = "current_thread")]
async fn finish_first_spill_throws_synchronously_after_flushing_the_decoder() {
    // Frozen output-accumulator.ts:82-90: finish commits finished/decoder state before ensureTempFile.
    assert_first_spill_path_error("finish").await;
}

#[tokio::test(flavor = "current_thread")]
async fn snapshot_first_spill_throws_synchronously_and_retains_the_first_path() {
    // Frozen output-accumulator.ts:113-114,213-219: persistence assigns path before constructor validation.
    assert_first_spill_path_error("snapshot").await;
}

async fn assert_first_spill_path_error(mode: &str) {
    let mut oracle = oracle::TsOracle::spawn(
        "output-accumulator-path-error-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    let expected = oracle.request(&json!({"id": 27009, "mode": mode}));
    assert_eq!(expected["ok"], true, "{expected}");
    let expected = &expected["output"];
    assert_eq!(expected["thrown"]["name"], "TypeError");
    assert_eq!(expected["thrown"]["code"], "ERR_INVALID_ARG_VALUE");
    assert_eq!(expected["close"], "resolved");
    let path = expected["snapshot"]["fullOutputPath"].as_str().unwrap();
    assert!(path.contains('\0'));
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_bytes: 1.0,
            max_lines: if mode == "snapshot" { -1.0 } else { 2000.0 },
            ..Default::default()
        },
        path,
    );
    let result = match mode {
        "finish" => {
            output
                .append(&[0xe2])
                .expect("incomplete UTF-8 stays below both byte limits");
            output.finish()
        }
        "snapshot" => output
            .snapshot(SnapshotOptions {
                persist_if_truncated: true,
            })
            .map(|_| ()),
        "append" => output.append(b"payload"),
        _ => unreachable!("test modes are fixed"),
    };
    let error: NativeNodeError =
        result.expect_err("path validation propagates synchronously, not as a deferred stream error");
    output.finish().unwrap();
    let snapshot = output.snapshot(SnapshotOptions::default()).unwrap();
    let bytes =
        pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::to_value(&snapshot.truncation).unwrap());
    output
        .close_temp_file()
        .expect("no write stream exists after synchronous constructor validation fails");
    let actual: Value = json!({"thrown": error.value(), "snapshot": {
        "content": snapshot.content, "truncation": snapshot.truncation, "fullOutputPath": snapshot.full_output_path,
    }, "truncationJson": bytes, "close": "resolved"});
    assert_eq!(JsValue::from(actual), JsValue::from(expected.clone()));
}
