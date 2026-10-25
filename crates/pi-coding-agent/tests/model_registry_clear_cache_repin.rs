use crate::shared_oracle as oracle;
use pi_coding_agent::core::{model_registry::clear_api_key_cache, resolve_config_value};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

#[tokio::test]
async fn registry_clear_alias_invalidates_the_shared_command_cache_like_frozen() {
    let directory = tempfile::tempdir().unwrap();
    let source_witness = directory.path().join("source");
    let native_witness = directory.path().join("native");
    let source_command = format!("!printf x >> '{}'; printf fixture-value", source_witness.display());
    let native_command = JsString::from(format!(
        "!printf x >> '{}'; printf fixture-value",
        native_witness.display()
    ));
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let expected=source.request(&js_json!({"operation":"clear_alias_snapshot","command":source_command,"witness":source_witness.to_str().unwrap()}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(
        expected["output"],
        js_json!({"values":["fixture-value","fixture-value","fixture-value"],"before":1,"after":2})
    );
    let first = resolve_config_value(&native_command, None);
    let second = resolve_config_value(&native_command, None);
    let before = std::fs::read(&native_witness).unwrap().len();
    clear_api_key_cache();
    let third = resolve_config_value(&native_command, None);
    let after = std::fs::read(&native_witness).unwrap().len();
    assert_eq!(
        js_json!({"values":[first,second,third],"before":before,"after":after}),
        expected["output"]
    );
}
