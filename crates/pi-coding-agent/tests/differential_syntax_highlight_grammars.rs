#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::utils::syntax_highlight::{HighlightOptions, highlight_html};
use serde_json::{Value, json};

const GRAMMARS: &str = include_str!("../src/utils/syntax_highlight/grammars.json");

/// Multi-construct snippet exercised against every generated grammar.
const GENERIC_SNIPPET: &str = r##"/* block comment */
// line comment
# hash comment
-- dash comment
; semicolon comment
def function_name(arg1, arg2):
    value = 42
    text = "double \"quoted\" string"
    other = 'single \'quoted\' string'
    if value >= 3.14 and not arg1:
        return {"key": [1, 2, 3], "nested": true}
class Example extends Base {
  method() { return this.value + 1; }
}
<p class="tag">text &amp; more</p>
@decorator
$x = %y + &z
"##;

fn grammar_keys() -> Vec<String> {
    let file: Value = serde_json::from_str(GRAMMARS).expect("grammars");
    file["languages"]
        .as_array()
        .expect("languages")
        .iter()
        .map(|language| language["key"].as_str().expect("key").to_owned())
        .collect()
}

fn request_cases(keys: &[String]) -> Vec<Value> {
    keys.iter()
        .map(|key| {
            json!({
                "kind": "raw",
                "name": key,
                "code": GENERIC_SNIPPET,
                "language": key,
                "ignoreIllegals": false,
            })
        })
        .collect()
}

fn actual_case(key: &str) -> Value {
    let html = highlight_html(
        GENERIC_SNIPPET,
        &HighlightOptions {
            language: Some(key.to_owned()),
            ignore_illegals: Some(false),
            ..HighlightOptions::default()
        },
    );
    json!({"name": key, "value": html})
}

#[test]
fn every_generated_grammar_matches_the_pinned_highlight_js() {
    let keys = grammar_keys();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-syntax-highlight-oracle.ts",
        "packages/coding-agent/src/utils/syntax-highlight.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_007,
        "operation": "coding_agent_syntax_highlight",
        "cases": request_cases(&keys),
    }));
    assert_eq!(expected["ok"], Value::Bool(true), "{expected}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    assert_eq!(expected_results.len(), keys.len());

    let mut failures = Vec::new();
    for (key, expected) in keys.iter().zip(expected_results) {
        let actual = actual_case(key);
        if actual != *expected {
            failures.push(format!(
                "{key}\n  rust:   {}\n  oracle: {}",
                actual["value"], expected["value"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} grammar mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
