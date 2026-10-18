use indexmap::IndexMap;

use pi_ai::model_catalog::flatten_model_catalog;
use pi_ai::types::{Modality, Model, ModelCost, ModelCostTier};
use serde_json::json;

fn model(id: &str, provider: &str, name: &str) -> Model {
    Model {
        id: id.to_owned(),
        name: name.to_owned(),
        api: "openai-completions".to_owned(),
        provider: provider.to_owned(),
        base_url: "https://example.invalid".to_owned(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            rates: pi_ai::types::ModelCostRates {
                input: 1.0,
                output: 2.0,
                cache_read: 0.1,
                cache_write: 0.2,
            },
            tiers: Some(vec![ModelCostTier {
                rates: pi_ai::types::ModelCostRates {
                    input: 2.0,
                    output: 4.0,
                    cache_read: 0.2,
                    cache_write: 0.4,
                },
                input_tokens_above: 200_000.0,
            }]),
        },
        context_window: 128_000.0,
        auto_compaction_threshold: None,
        max_tokens: 16_000.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

#[test]
fn flattens_groups_and_keeps_the_last_value_for_duplicate_ids() {
    let groups = vec![
        IndexMap::from([
            ("alpha".to_owned(), model("alpha", "provider", "Alpha")),
            ("shared".to_owned(), model("shared", "provider", "First")),
        ]),
        IndexMap::from([
            ("beta".to_owned(), model("beta", "provider", "Beta")),
            ("shared".to_owned(), model("shared", "provider", "Second")),
        ]),
    ];
    let catalog = flatten_model_catalog(groups);
    assert_eq!(
        catalog.keys().cloned().collect::<Vec<_>>(),
        ["alpha", "shared", "beta"],
        "later duplicates replace values without moving the original key"
    );
    assert_eq!(catalog["shared"].name, "Second");
    assert_eq!(catalog["beta"].id, "beta");
}

#[test]
fn empty_catalog_flattens_to_an_empty_map() {
    assert!(flatten_model_catalog(Vec::<IndexMap<String, Model>>::new()).is_empty());
}

#[test]
fn model_cost_tiers_serialize_like_generated_data() {
    let value = serde_json::to_value(model("alpha", "provider", "Alpha")).expect("serializes");
    assert_eq!(
        value["cost"],
        json!({
            "input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 0.2,
            "tiers": [{"input": 2, "output": 4, "cacheRead": 0.2, "cacheWrite": 0.4, "inputTokensAbove": 200000}]
        })
    );
}
