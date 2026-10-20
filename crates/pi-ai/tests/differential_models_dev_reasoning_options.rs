#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::models_dev_reasoning_options::{ModelsDevReasoningOption, get_effort_thinking_level_map};
use serde_json::{Value, json};

fn assert_parity(oracle: &mut oracle::TsOracle, options: Value) {
    let typed: Vec<ModelsDevReasoningOption> = serde_json::from_value(options.clone()).unwrap();
    let actual = get_effort_thinking_level_map(&typed);
    let expected = oracle.request(&options);
    let actual_json = match actual {
        Some(map) => json!({"json": serde_json::to_string(&map).unwrap()}),
        None => json!({}),
    };
    assert_eq!(actual_json, expected, "options: {options}");
}

#[test]
fn pinned_source_matches_every_effort_subset_order_and_option_combination() {
    let mut oracle = oracle::TsOracle::spawn(
        "models-dev-reasoning-options-oracle.ts",
        "packages/ai/scripts/models-dev-reasoning-options.ts",
    );
    let values = json!([
        "none", "minimal", "low", "medium", "high", "xhigh", "max", "default", null
    ]);
    let values = values.as_array().unwrap();
    for mask in 0..(1 << values.len()) {
        let subset: Vec<_> = values
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, value)| value.clone())
            .collect();
        assert_parity(&mut oracle, json!([{"type":"effort","values":subset}]));
        let reversed: Vec<_> = subset.iter().rev().cloned().collect();
        assert_parity(
            &mut oracle,
            json!([
                {"type":"toggle"},
                {"type":"effort","values":reversed},
                {"type":"budget_tokens","min":1024,"max":32000},
                {"type":"effort","values":subset},
                {"type":"effort","values":[]}
            ]),
        );
    }
    for first in values {
        for second in values {
            assert_parity(
                &mut oracle,
                json!([
                    {"type":"effort","values":[first]},
                    {"type":"effort","values":[second,first]},
                    {"type":"toggle"}
                ]),
            );
        }
    }
    for options in [
        json!([]),
        json!([{"type":"toggle"}]),
        json!([{"type":"budget_tokens"}]),
        json!([{"type":"budget_tokens","min":-1.5}]),
        json!([{"type":"budget_tokens","max":32000.5},{"type":"toggle"}]),
    ] {
        assert_parity(&mut oracle, options);
    }
}
