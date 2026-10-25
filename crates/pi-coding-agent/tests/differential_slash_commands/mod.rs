use crate::shared_oracle as oracle;

use pi_coding_agent::core::slash_commands::builtin_slash_commands;
use serde_json::{Value, json};

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-slash-commands-oracle.ts",
        "packages/coding-agent/src/core/slash-commands.ts",
    );
    let response = oracle.request(&json!({"id": 1}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let expected = response["output"]["commands"]
        .as_array()
        .expect("oracle commands must be an array");
    let actual: Vec<Value> = builtin_slash_commands()
        .into_iter()
        .map(|command| {
            json!({
                "name": command.name,
                "description": command.description,
                "argumentHint": command.argument_hint,
            })
        })
        .collect();

    assert_eq!(Value::Array(actual), Value::Array(expected.clone()));
}
