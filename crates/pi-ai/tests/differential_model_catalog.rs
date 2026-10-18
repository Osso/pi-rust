#[path = "support/oracle.rs"]
mod oracle;

use indexmap::IndexMap;

use pi_ai::model_catalog::flatten_model_catalog;
use pi_ai::types::Model;
use serde_json::{Value, json};

fn model(id: &str, provider: &str, name: &str, api: &str) -> Value {
    json!({
        "id": id, "name": name, "api": api, "provider": provider,
        "baseUrl": "https://example.invalid", "reasoning": false, "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 128000, "maxTokens": 16000
    })
}

fn requests() -> Vec<Value> {
    vec![
        json!({"id": 0, "groups": [
            {"alpha": model("alpha", "provider", "Alpha", "openai-completions")},
            {"beta": model("beta", "provider", "Beta", "openai-responses")}
        ]}),
        json!({"id": 1, "groups": [
            {"alpha": model("alpha", "provider", "Alpha", "openai-completions"),
             "shared": model("shared", "provider", "First", "openai-completions")},
            {"shared": model("shared", "provider", "Second", "anthropic-messages")}
        ]}),
        json!({"id": 2, "groups": []}),
        json!({"id": 3, "groups": [{}]}),
        json!({"id": 4, "groups": [
            {"zeta": model("zeta", "provider", "Zeta", "openai-completions"),
             "alpha": model("alpha", "provider", "Alpha", "openai-completions"),
             "shared": model("shared", "provider", "First", "openai-completions")},
            {"alpha": model("alpha", "provider", "Alpha v2", "anthropic-messages"),
             "middle": model("middle", "provider", "Middle", "openai-responses")}
        ]}),
    ]
}

fn observe(request: &Value) -> Value {
    let groups: Vec<IndexMap<String, Model>> = request["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .map(|group| serde_json::from_value(group.clone()).expect("group"))
        .collect();
    json!({"id": request["id"], "ok": true, "output": {"catalog": flatten_model_catalog(groups)}})
}

#[test]
fn model_catalog_flattening_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("model-catalog-oracle.ts", "packages/ai/src/model-catalog.ts");
    for request in requests() {
        assert_eq!(observe(&request), oracle.request(&request), "request {request}");
    }
}

#[test]
fn model_catalog_preserves_source_key_order() {
    let mut oracle = oracle::TsOracle::spawn("model-catalog-oracle.ts", "packages/ai/src/model-catalog.ts");
    let request = &requests()[4];
    let observed = observe(request);
    let expected = oracle.request(request);
    let observed_keys: Vec<String> = observed["output"]["catalog"]
        .as_object()
        .expect("catalog object")
        .keys()
        .cloned()
        .collect();
    let expected_keys: Vec<String> = expected["output"]["catalog"]
        .as_object()
        .expect("catalog object")
        .keys()
        .cloned()
        .collect();
    // Later duplicates replace the value without moving the original key position.
    assert_eq!(observed_keys, vec!["zeta", "alpha", "shared", "middle"]);
    assert_eq!(observed_keys, expected_keys);
    assert_eq!(observed["output"]["catalog"]["alpha"]["name"], json!("Alpha v2"));
}
