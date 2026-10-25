use crate::shared_oracle as oracle;

use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::grep::GrepTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

#[test]
fn coding_agent_grep_tool_matches_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    std::fs::write(
        temp.path().join("a.txt"),
        "alpha-match\nsecond line\nalpha-match again\n",
    )
    .expect("a.txt");
    std::fs::write(temp.path().join("b.md"), "beta-match in markdown\n").expect("b.md");
    std::fs::write(temp.path().join("ignored.txt"), "ignored-match\n").expect("ignored");
    std::fs::write(temp.path().join(".hidden.txt"), "hidden-match\n").expect("hidden");
    std::fs::write(temp.path().join(".gitignore"), "ignored.txt\n").expect("gitignore");
    std::fs::create_dir(temp.path().join("sub")).expect("sub dir");
    std::fs::write(temp.path().join("sub/c.txt"), "SUB-MATCH in sub\n").expect("c.txt");
    let long_line = format!("long-match {}\n", "x".repeat(600));
    std::fs::write(temp.path().join("long.txt"), long_line).expect("long.txt");

    let cases = vec![
        json!({"pattern": "hidden-match"}),
        json!({"pattern": "ignored-match"}),
        json!({"pattern": "beta-match", "glob": "*.md"}),
        json!({"pattern": "sub-match", "ignoreCase": true, "path": "sub"}),
        json!({"pattern": "alpha-match", "literal": true, "path": "a.txt"}),
        json!({"pattern": "alpha-match", "context": 1, "path": "a.txt"}),
        json!({"pattern": "alpha-match", "limit": 1, "path": "a.txt"}),
        json!({"pattern": "alpha-match", "path": "a.txt"}),
        json!({"pattern": "long-match", "path": "long.txt"}),
        json!({"pattern": "absent-pattern"}),
        json!({"pattern": "alpha-match", "path": "missing"}),
        json!({"pattern": "-match", "path": "a.txt"}),
        json!({"pattern": "beta-match", "glob": "*.txt"}),
    ];

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-grep-tool-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_500,
        "operation": "coding_agent_grep_tool",
        "cwd": cwd,
        "cases": cases,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");

    let tool = GrepTool::new(&cwd);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (params, expected)) in cases.iter().zip(expected_results).enumerate() {
        let actual = match runtime.block_on(tool.execute(
            "grep-differential".to_owned(),
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
