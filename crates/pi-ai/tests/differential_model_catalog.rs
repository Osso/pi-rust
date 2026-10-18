#[path = "support/oracle.rs"]
mod oracle;

use std::collections::BTreeMap;

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
    ]
}

fn observe(request: &Value) -> Value {
    let groups: Vec<BTreeMap<String, Model>> = request["groups"]
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
