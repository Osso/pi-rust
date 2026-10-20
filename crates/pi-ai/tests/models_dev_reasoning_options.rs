use pi_ai::models_dev_reasoning_options::{ModelsDevEffort, ModelsDevReasoningOption, get_effort_thinking_level_map};
use pi_ai::types::ModelThinkingLevel;
use serde_json::json;

#[test]
fn mixed_options_emit_source_order_and_explicit_nulls() {
    let options = vec![
        ModelsDevReasoningOption::Toggle,
        ModelsDevReasoningOption::BudgetTokens {
            min: Some(1024.0),
            max: Some(32000.0),
        },
        ModelsDevReasoningOption::Effort {
            values: vec![Some(ModelsDevEffort::Max), Some(ModelsDevEffort::Low)],
        },
        ModelsDevReasoningOption::Effort {
            values: vec![
                Some(ModelsDevEffort::None),
                Some(ModelsDevEffort::High),
                Some(ModelsDevEffort::Low),
            ],
        },
    ];
    let map = get_effort_thinking_level_map(&options).expect("usable efforts");
    assert_eq!(
        serde_json::to_string(&map).unwrap(),
        r#"{"off":"none","minimal":null,"low":"low","medium":null,"high":"high","xhigh":null,"max":"max"}"#
    );
    assert!(!map.contains_key(&ModelThinkingLevel::Ultra));
}

#[test]
fn absent_or_unusable_efforts_return_none() {
    for options in [
        vec![],
        vec![ModelsDevReasoningOption::Toggle],
        vec![ModelsDevReasoningOption::BudgetTokens { min: None, max: None }],
        vec![ModelsDevReasoningOption::Effort { values: vec![] }],
        vec![ModelsDevReasoningOption::Effort {
            values: vec![None, Some(ModelsDevEffort::Default)],
        }],
    ] {
        assert_eq!(get_effort_thinking_level_map(&options), None);
    }
}

#[test]
fn none_only_and_max_only_keep_distinct_off_values() {
    for (effort, expected) in [
        (
            ModelsDevEffort::None,
            r#"{"off":"none","minimal":null,"low":null,"medium":null,"high":null,"xhigh":null,"max":null}"#,
        ),
        (
            ModelsDevEffort::Max,
            r#"{"off":null,"minimal":null,"low":null,"medium":null,"high":null,"xhigh":null,"max":"max"}"#,
        ),
    ] {
        let map = get_effort_thinking_level_map(&[ModelsDevReasoningOption::Effort {
            values: vec![Some(effort)],
        }]);
        assert_eq!(serde_json::to_string(&map).unwrap(), expected);
    }
}

#[test]
fn option_union_preserves_wire_values_and_optional_budget_bounds() {
    for fixture in [
        json!({"type":"toggle"}),
        json!({"type":"effort","values":["none","minimal","low","medium","high","xhigh","max","default",null]}),
        json!({"type":"budget_tokens"}),
        json!({"type":"budget_tokens","min":-1.5}),
        json!({"type":"budget_tokens","max":32000.5}),
        json!({"type":"budget_tokens","min":1024.0,"max":32000.0}),
    ] {
        let option: ModelsDevReasoningOption = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(option).unwrap(), fixture);
    }
    for invalid in [
        json!({"type":"effort","values":["ultra"]}),
        json!({"type":"effort","values":[1]}),
    ] {
        assert!(serde_json::from_value::<ModelsDevReasoningOption>(invalid).is_err());
    }
}
