#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::utils::validation::validate_arguments;
use pi_coding_agent::core::tools::search_current_session_history::parameters;
use pi_js_compat::js_json;
use serde_json::{Value, json};

#[test]
fn real_history_tool_nested_integer_coercion_matches_frozen() {
    let inputs = json!([
        {"query":"retry", "limit":"10", "context_entries":"2"},
        {"query":"retry", "limit":"10.9", "context_entries":"2.5"},
        {"query":"retry", "limit":"0"},
        {"query":"retry", "context_entries":"11"},
        {"query":"retry", "limit":null},
        {"query":"retry", "limit":"wrong"}
    ]);
    let mut oracle = oracle::TsOracle::spawn(
        "validation-history-tool-oracle.mts",
        "packages/ai/src/utils/validation.ts",
    );
    let expected = oracle.request(&json!({"id":1, "arguments":inputs}));
    assert_eq!(expected["ok"], true, "{expected}");
    let schema = js_json::to_value(parameters()).unwrap();
    let actual: Vec<Value> = inputs
        .as_array()
        .unwrap()
        .iter()
        .map(|args| {
            let args = js_json::to_value(args).unwrap();
            match validate_arguments(&schema, &args, true, "search_current_session_history") {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"error":{"name":"Error","message":error.message()}}),
            }
        })
        .collect();
    assert_eq!(Value::Array(actual), expected["results"]);
}
