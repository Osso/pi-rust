use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};

pub fn assert_from_frozen_builder(actual: &Value, custom_prompt: &Value, cwd: &JsString, clock_bounds: [f64; 2]) {
    let mut source = crate::oracle::TsOracle::spawn(
        "coding-agent-system-prompt-oracle.ts",
        "packages/coding-agent/src/core/system-prompt.ts",
    );
    let mut expected_prompts = Vec::new();
    for now in clock_bounds {
        let expected = source.request(&json!({
            "id":"sdk-example", "nowMillis":now, "options":{"customPrompt":custom_prompt,"cwd":cwd},
        }));
        assert_eq!(expected["ok"], true);
        expected_prompts.push(JsValue::from(expected["output"]["prompt"].clone()));
    }
    let actual = JsValue::from(actual.clone());
    assert!(
        expected_prompts.contains(&actual),
        "actual={actual:?}; frozen={expected_prompts:?}"
    );
}
