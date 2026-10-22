#[path = "support/oracle.rs"]
mod oracle;

use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::read::ReadTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

#[test]
fn coding_agent_read_tool_matches_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_string_lossy().into_owned();
    let fit = temp.path().join("fit.txt");
    std::fs::write(&fit, "Hello, world!\nLine 2\nLine 3").expect("fit fixture");
    let numbered = temp.path().join("numbered.txt");
    std::fs::write(
        &numbered,
        (0..100)
            .map(|index| format!("Line {}", index + 1))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("numbered fixture");
    let many_lines = temp.path().join("many-lines.txt");
    std::fs::write(&many_lines, vec!["x"; 2500].join("\n")).expect("line fixture");
    let many_bytes = temp.path().join("many-bytes.txt");
    std::fs::write(
        &many_bytes,
        (0..500)
            .map(|index| format!("Line {}: {}", index + 1, "x".repeat(200)))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .expect("byte fixture");
    let short = temp.path().join("short.txt");
    std::fs::write(&short, "Line 1\nLine 2\nLine 3").expect("short fixture");

    let cases = vec![
        json!({"path": fit.to_string_lossy()}),
        json!({"path": numbered.to_string_lossy(), "offset": 51}),
        json!({"path": numbered.to_string_lossy(), "limit": 10}),
        json!({"path": numbered.to_string_lossy(), "offset": 41, "limit": 20}),
        json!({"path": numbered.to_string_lossy(), "offset": 100}),
        json!({"path": numbered.to_string_lossy(), "offset": 101}),
        json!({"path": many_lines.to_string_lossy()}),
        json!({"path": many_bytes.to_string_lossy()}),
        json!({"path": many_lines.to_string_lossy(), "limit": 5000}),
        json!({"path": short.to_string_lossy(), "offset": 100}),
        json!({"path": temp.path().join("missing.txt").to_string_lossy()}),
    ];

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-read-tool-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_200,
        "operation": "coding_agent_read_tool",
        "cwd": cwd,
        "cases": cases,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");

    let tool = ReadTool::new(&cwd);
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (params, expected)) in cases.iter().zip(expected_results).enumerate() {
        let actual = match runtime.block_on(tool.execute(
            "read-differential".to_owned(),
            params.clone(),
            None,
            Arc::new(|_result| {}),
            None,
        )) {
            Ok(result) => json!({"ok": true, "value": result}),
            Err(error) => json!({"ok": false, "error": error.message}),
        };
        assert_eq!(&actual, expected, "case {index}");
    }
}
