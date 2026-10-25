use crate::shared_harness_support as support;

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::prompt_templates::{load_prompt_templates, parse_command_args, substitute_args};
use pi_js_compat::js_json::JsString;
use serde_json::json;

#[test]
fn argument_operations_preserve_unpaired_units_against_frozen() {
    let input = JsString::from_utf16(vec![0xd800, 0x20, 0x27, 0xdc00, 0x27]);
    let content = JsString::from_utf16(vec![0xdfff, 0x5b, 0x24, 0x31, 0x5d, 0x24, 0x40]);
    let args = parse_command_args(&input);
    let output = substitute_args(&content, &args);
    let mut oracle = support::spawn_oracle();
    let response = oracle.request(&support::oracle_request(
        2,
        "argument_units",
        json!({
            "argsString": input.units(), "contentUnits": content.units(),
        }),
    ));
    assert_eq!(
        json!({ "args": args.iter().map(JsString::units).collect::<Vec<_>>(), "output": output.units() }),
        *support::expect_ok(&response)
    );
}

#[tokio::test]
async fn description_slice_preserves_split_surrogate_against_frozen() {
    let rust_root = tempfile::tempdir().unwrap();
    let ts_root = tempfile::tempdir().unwrap();
    let content = format!("{}😀tail", "a".repeat(59));
    std::fs::write(rust_root.path().join("split.md"), &content).unwrap();
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: rust_root.path().to_str().unwrap().into(),
        shell_path: None,
        shell_env: Default::default(),
    });
    let loaded = load_prompt_templates(&env, &[&JsString::from("split.md")])
        .await
        .unwrap();
    let actual = loaded.prompt_templates[0].description.clone().unwrap();
    let mut oracle = support::spawn_oracle();
    let response = oracle.request(&support::oracle_request(
        1,
        "prompt_template_units",
        json!({
            "fixture": {"root": ts_root.path(), "files": {"split.md": content}, "paths": ["split.md"]},
        }),
    ));
    let expected = support::expect_ok(&response);
    assert_eq!(json!([actual.units()]), *expected);
    assert_eq!(actual.units()[59], 0xd83d);
}
