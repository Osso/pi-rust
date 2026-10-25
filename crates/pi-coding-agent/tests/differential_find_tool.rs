use crate::shared_oracle as oracle;

use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::find::FindTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

#[test]
fn coding_agent_find_tool_matches_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    std::fs::write(temp.path().join("visible.txt"), "x").expect("visible");
    std::fs::write(temp.path().join("only.md"), "x").expect("markdown");
    std::fs::write(temp.path().join(".hidden-only.txt"), "x").expect("hidden");
    std::fs::write(temp.path().join("ignored.txt"), "x").expect("ignored");
    std::fs::write(temp.path().join(".gitignore"), "ignored.txt\n").expect("gitignore");
    std::fs::create_dir(temp.path().join("sub")).expect("sub dir");
    std::fs::write(temp.path().join("sub/nested.txt"), "x").expect("nested");

    let cases = vec![
        json!({"pattern": "visible.txt"}),
        json!({"pattern": "*.md"}),
        json!({"pattern": "*hidden-only*"}),
        json!({"pattern": "ignored.txt"}),
        json!({"pattern": "sub/*.txt"}),
        json!({"pattern": "missing-*.txt"}),
        json!({"pattern": "visible.txt", "limit": 1}),
        json!({"pattern": "nested.txt", "path": "sub"}),
        json!({"pattern": "["}),
    ];

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-find-tool-oracle.ts",
        "packages/coding-agent/src/core/tools/find.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_600,
        "operation": "coding_agent_find_tool",
        "cwd": cwd,
        "cases": cases,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");

    let tool = FindTool::new(&cwd);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (params, expected)) in cases.iter().zip(expected_results).enumerate() {
        let actual = match runtime.block_on(tool.execute(
            "find-differential".to_owned(),
            params.clone(),
            None,
            Arc::new(|_result| Ok(())),
            None,
        )) {
            Ok(result) => json!({"ok": true, "value": result}),
            Err(error) => json!({"ok": false, "error": error.message}),
        };
        assert_eq!(&actual, expected, "case {index}");
    }
}
