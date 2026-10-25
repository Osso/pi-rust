use crate::shared_oracle as oracle;

use pi_coding_agent::config::{self, InstallContext};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use std::collections::BTreeMap;

#[test]
fn configured_npm_probe_preserves_node_spawn_argument_rejections() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    std::fs::create_dir(&package).unwrap();
    let context = InstallContext {
        package_dir: package.clone(),
        exec_path: "/fixture/npm/node".into(),
        entrypoint: None,
        is_bun_binary: false,
        is_bun_runtime: false,
        platform: "linux".into(),
        home: temp.path().to_owned(),
        path_env: Some(temp.path().display().to_string()),
        env: BTreeMap::new(),
    };
    let mut oracle = oracle::TsOracle::spawn("config-oracle.ts", "packages/coding-agent/src/config.ts");
    let long_argument = JsString::from("x".repeat(115)) + JsString::from_utf16(vec![0xd800, 0]) + "tail";
    let cases = [
        vec![JsString::from("")],
        vec![JsString::from("bad\0file")],
        vec![JsString::from("/fixture/unused"), JsString::from("bad\0argument")],
        vec![JsString::from("/fixture/unused"), long_argument],
    ];
    let mut mismatches = Vec::new();
    for (id, command) in cases.into_iter().enumerate() {
        let expected: JsValue = oracle.request(&json!({
            "id":id,"op":"install","platform":"linux","execPath":context.exec_path,
            "argv1":null,"npmCommand":command,
            "env":{"PI_PACKAGE_DIR":package,"PATH":context.path_env}
        }));
        assert_eq!(expected["ok"], JsValue::Bool(false), "{expected:?}");
        let result = config::get_self_update_command(&context, &"package".into(), Some(&command), None);
        let actual = match result {
            Ok(value) => json!({"unexpectedSuccess":value.is_some()}),
            Err(error) => {
                let native = error.into_native_error();
                let mut value = json!({"name":native.name,"message":native.message});
                if let Some(code) = native.value().get("code") {
                    value.as_object_mut().unwrap().insert("code".into(), code.clone());
                }
                value
            }
        };
        if actual != expected["error"] {
            mismatches.push(format!("case {id}: {actual:?}; expected {:?}", expected["error"]));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
