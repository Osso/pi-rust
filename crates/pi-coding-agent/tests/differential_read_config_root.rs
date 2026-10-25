use crate::shared_oracle as oracle;

use pi_coding_agent::core::tools::read::create_agent_config_read_tool_definition;
use pi_js_compat::js_json::{JsValue, to_value};
use serde_json::{Value, json};

#[tokio::test]
async fn config_root_reads_use_canonical_paths_and_reject_escapes_before_access() {
    let cwd = tempfile::tempdir().unwrap();
    let root = cwd.path().join("allowed");
    let sibling = cwd.path().join("allowed-sibling");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&sibling).unwrap();
    std::fs::write(root.join("inside.txt"), "inside").unwrap();
    std::fs::write(sibling.join("outside.txt"), "outside").unwrap();
    let alias = cwd.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    std::os::unix::fs::symlink(sibling.join("outside.txt"), root.join("escape.txt")).unwrap();
    std::os::unix::fs::symlink(root.join("inside.txt"), root.join("inside-link.txt")).unwrap();
    let cases = vec![
        json!({"root":root,"path":"allowed/inside.txt"}),
        json!({"root":root,"path":"allowed/inside-link.txt"}),
        json!({"root":alias,"path":"alias/inside.txt"}),
        json!({"root":root,"path":"allowed/escape.txt"}),
        json!({"root":root,"path":"allowed-sibling/outside.txt"}),
        json!({"root":root,"path":"allowed/../allowed-sibling/outside.txt"}),
        json!({"root":root,"path":"allowed/missing.txt"}),
        json!({"root":cwd.path().join("missing-root"),"path":"allowed/inside.txt"}),
        json!({"root":"","path":"allowed-sibling/outside.txt"}),
        json!({"root":root,"path":"allowed/nul\0name"}),
    ];
    let mut oracle = oracle::TsOracle::spawn(
        "read-config-root-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let expected: Value = oracle.request(&json!({"id":82,"cwd":cwd.path(),"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected}");
    let mut mismatches = Vec::new();
    for (case, expected) in cases.iter().zip(expected["output"].as_array().unwrap()) {
        let definition =
            create_agent_config_read_tool_definition(cwd.path().to_str().unwrap(), case["root"].as_str().unwrap());
        let result = (definition.execute)(
            "root-read".into(),
            JsValue::from(json!({"path":case["path"]})),
            None,
            None,
            None,
        )
        .await;
        let actual = match result {
            Ok(result) => json!({"value":to_value(&result).unwrap().to_serde().unwrap()}),
            Err(error) => json!({"error":error.message}),
        };
        if actual != *expected {
            mismatches.push(format!("{case}: {actual} != {expected}"));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
