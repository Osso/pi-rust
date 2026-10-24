#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_js_compat::js_json::JsValue;
use pi_js_compat::node_error::NativeNodeError;
use serde_json::{Value, json};

#[tokio::test(flavor = "current_thread")]
async fn nul_path_throws_synchronously_and_retains_the_first_path_without_a_stream() {
    let mut oracle = oracle::TsOracle::spawn(
        "output-accumulator-path-error-oracle.ts",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
    );
    let expected = oracle.request(&json!({"id": 27009}));
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
            ..Default::default()
        },
        path,
    );
    let error: NativeNodeError = output
        .append(b"payload")
        .expect_err("path validation propagates synchronously, not as a deferred stream error");
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
