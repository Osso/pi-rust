use crate::shared_oracle as oracle;

use std::sync::Arc;

use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::bash::{BashTool, BashToolOptions};
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::js_regex::JsRegex as Regex;
use pi_js_compat::{js_json, js_json as json};

#[test]
fn coding_agent_bash_tool_matches_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    let cases = vec![
        json!({"command": "echo 'test output'"}),
        json!({"command": "printf 'a\\r\\nb\\n'"}),
        json!({"command": "printf 'red\\n'; exit 3"}),
        json!({"command": "seq 1 2500"}),
        json!({"command": "echo $PREFIX_VAR", "prefix": "export PREFIX_VAR=hello"}),
        json!({"command": "true", "timeout": 0}),
        json!({"command": "echo hi", "timeout": 2147484}),
    ];

    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-bash-tool-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_800,
        "operation": "coding_agent_bash_tool",
        "cwd": cwd,
        "cases": cases,
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");

    let path_pattern = Regex::new(r"\S*pi-bash\S*\.log").expect("path regex");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (case, expected)) in cases.iter().zip(expected_results).enumerate() {
        let tool = BashTool::with_options(
            &cwd,
            BashToolOptions {
                command_prefix: case["prefix"].as_string().cloned(),
                ..Default::default()
            },
        );
        let params = json!({
            "command": case["command"],
            "timeout": case.get("timeout").cloned().unwrap_or(Value::Null),
        });
        let actual = match runtime.block_on(tool.execute(
            "bash-differential".to_owned(),
            params.clone(),
            None,
            Arc::new(|_result| Ok(())),
            None,
        )) {
            Ok(result) => json!({"ok": true, "value": result}),
            Err(error) => json!({"ok": false, "error": error.message}),
        };
        let actual = normalize_paths(&actual, &path_pattern);
        let expected = normalize_paths(expected, &path_pattern);
        if actual != expected {
            let actual_text = js_json::stringify_lossless(&actual);
            let expected_text = js_json::stringify_lossless(&expected);
            let position = actual_text
                .chars()
                .zip(expected_text.chars())
                .position(|(left, right)| left != right)
                .unwrap_or_else(|| actual_text.chars().count().min(expected_text.chars().count()));
            let actual_snippet = actual_text
                .chars()
                .skip(position.saturating_sub(60))
                .take(200)
                .collect::<String>();
            let expected_snippet = expected_text
                .chars()
                .skip(position.saturating_sub(60))
                .take(200)
                .collect::<String>();
            panic!("case {index} differs at char {position}\nactual: {actual_snippet}\nexpected: {expected_snippet}");
        }
    }
}

fn normalize_paths(value: &Value, pattern: &Regex) -> Value {
    match value {
        Value::String(text) => Value::String(pattern.replace_all(text, "<PATH>")),
        Value::Array(values) => Value::Array(values.iter().map(|value| normalize_paths(value, pattern)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), normalize_paths(value, pattern)))
                .collect(),
        ),
        other => other.clone(),
    }
}
