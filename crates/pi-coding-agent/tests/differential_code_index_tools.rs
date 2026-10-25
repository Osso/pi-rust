use crate::shared_oracle as oracle;

use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::code_index::{OutlineTool, ReferencesTool, SymbolTool};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

#[test]
fn coding_agent_code_index_tools_match_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    let source = temp.path().join("sample.rs");
    std::fs::write(&source, "pub fn alpha() {}\n\nstruct Beta;\n").expect("sample.rs");

    let cases = vec![
        json!({"kind": "outline", "params": {"path": source.to_str().expect("utf-8 test path")}}),
        json!({"kind": "outline", "params": {"path": source.to_str().expect("utf-8 test path"), "show": "alpha"}}),
        json!({"kind": "symbol", "params": {"name": "alpha"}}),
        json!({"kind": "symbol", "params": {"name": "alpha", "kind": "function"}}),
        json!({"kind": "symbol", "params": {"name": "missing"}}),
        json!({"kind": "references", "params": {"name": "alpha"}}),
        json!({"kind": "references", "params": {"name": "alpha", "kind": "call"}}),
        json!({"kind": "references", "params": {"name": "missing"}}),
        json!({"kind": "outline", "params": {"path": temp.path().join("missing.rs").to_str().expect("utf-8 test path")}}),
    ];

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-code-index-tools-oracle.ts",
        "packages/coding-agent/src/core/tools/code-index.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_700,
        "operation": "coding_agent_code_index_tools",
        "cwd": cwd,
        "cases": cases,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (case, expected)) in cases.iter().zip(expected_results).enumerate() {
        let kind = case["kind"].as_string().expect("kind");
        let params = case["params"].clone();
        let tool: Arc<dyn AgentTool> = if kind == "outline" {
            Arc::new(OutlineTool::new(&cwd))
        } else if kind == "symbol" {
            Arc::new(SymbolTool::new(&cwd))
        } else {
            Arc::new(ReferencesTool::new(&cwd))
        };
        let actual = match runtime.block_on(tool.execute(
            "code-index-differential".to_owned(),
            params,
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
