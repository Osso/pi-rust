use crate::shared_oracle as oracle;

use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::ls::LsTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

#[test]
fn coding_agent_ls_tool_matches_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    std::fs::write(temp.path().join("a.txt"), "a").expect("file");
    std::fs::write(temp.path().join(".hidden"), "h").expect("file");
    std::fs::write(temp.path().join("B.txt"), "b").expect("file");
    std::fs::create_dir(temp.path().join("zdir")).expect("dir");
    let empty = temp.path().join("empty");
    std::fs::create_dir(&empty).expect("empty dir");
    let many = temp.path().join("many");
    std::fs::create_dir(&many).expect("many dir");
    for index in 0..500 {
        std::fs::write(many.join(format!("file-{index:03}-xxxxxxxxxxxxxxxx")), "x").expect("file");
    }
    let small = temp.path().join("small");
    std::fs::create_dir(&small).expect("small dir");
    for index in 0..5 {
        std::fs::write(small.join(format!("file-{index}")), "x").expect("file");
    }
    let file = temp.path().join("a.txt");

    let cases = vec![
        json!({}),
        json!({"path": empty.to_str().expect("utf-8 test path")}),
        json!({"path": small.to_str().expect("utf-8 test path"), "limit": 2}),
        json!({"path": many.to_str().expect("utf-8 test path")}),
        json!({"path": temp.path().join("missing").to_str().expect("utf-8 test path")}),
        json!({"path": file.to_str().expect("utf-8 test path")}),
        json!({"path": "zdir"}),
    ];

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-ls-tool-oracle.ts",
        "packages/coding-agent/src/core/tools/ls.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_400,
        "operation": "coding_agent_ls_tool",
        "cwd": cwd,
        "cases": cases,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");

    let tool = LsTool::new(&cwd);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (params, expected)) in cases.iter().zip(expected_results).enumerate() {
        let actual = match runtime.block_on(tool.execute(
            "ls-differential".to_owned(),
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
