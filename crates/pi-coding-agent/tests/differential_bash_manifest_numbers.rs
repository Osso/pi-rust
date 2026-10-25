use crate::shared_oracle as oracle;
use pi_coding_agent::core::detached_bash_runner::{
    DetachedBashLaunchManifestData, write_detached_bash_launch_manifest,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};

// Frozen detached-bash-runner.ts:24-37,56-68 preserves number timeoutMs in the
// launch JSON and checksum; the bash launch adapter must not narrow milliseconds.
#[test]
fn detached_bash_manifest_preserves_fractional_and_existing_integer_timeouts() {
    let temp = tempfile::tempdir().unwrap();
    let baseline:DetachedBashLaunchManifestData=js_json::from_value(&json!({
        "activationPath":"activation","args":["-c","true"],"artifacts":{"directory":"job","outputPath":"job/output.log"},
        "command":"/bin/bash","controlDbPath":"control.sqlite","cwd":"/repo","env":{},"foregroundCompletionPath":"done.json", "runnerAddress":{"agentId":"job","sessionId":"main"},"sessionPath":"session.jsonl"
    })).unwrap();
    let mut cases = Vec::new();
    for timeout in [None, Some(0.0), Some(1.0), Some(1.25), Some(-1.25), Some(2147483647.0)] {
        let mut input = js_json::to_value(&baseline).unwrap();
        if let Some(timeout) = timeout {
            input["timeoutMs"] = json!(timeout);
        }
        cases.push(input);
    }
    let mut source = oracle::TsOracle::spawn(
        "bash-manifest-number-oracle.ts",
        "packages/coding-agent/src/core/detached-bash-runner.ts",
    );
    let expected = source.request(&json!({"id":28058,"cwd":temp.path().to_str().unwrap(),"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut failures = Vec::new();
    for (index, input) in cases.iter().enumerate() {
        let data = match js_json::from_value::<DetachedBashLaunchManifestData>(input) {
            Ok(data) => data,
            Err(error) => {
                failures.push(json!({"case":index,"deserializeError":error.to_string()}));
                continue;
            }
        };
        let path: JsString = temp
            .path()
            .join(format!("native-{index}.json"))
            .to_str()
            .unwrap()
            .into();
        write_detached_bash_launch_manifest(&path, &data).unwrap();
        let text = std::fs::read_to_string(path.to_utf8().unwrap()).unwrap();
        if expected["output"][index].as_string().unwrap() != text.as_str() {
            failures.push(json!({"case":index,"actual":text,"expected":expected["output"][index]}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}
