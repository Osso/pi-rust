use crate::shared_oracle as oracle;

use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use serde_json::{Value, json};

fn snapshot(registry: &ModelRegistry) -> Value {
    let model = registry.find("anthropic", "claude-opus-4-8").unwrap();
    json!({
        "configured": registry.has_configured_auth(&model).unwrap(),
        "available": registry.get_available().unwrap().iter().any(|item| item.provider == model.provider && item.id == model.id),
        "oauth": registry.is_using_oauth(&model),
    })
}

#[tokio::test]
async fn registry_observes_live_storage_without_a_refresh() {
    let mut oracle = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let expected = oracle.request(&json!({"id": 1, "auth": {}}));
    assert_eq!(expected["ok"], true, "{expected}");
    let storage = AuthStorage::in_memory(Vec::<(String, Value)>::new());
    let registry = ModelRegistry::in_memory(storage.clone()).unwrap();
    let before = snapshot(&registry);
    storage
        .set("anthropic", json!({"type": "api_key", "key": "q15-key"}))
        .unwrap();
    let after = snapshot(&registry);
    storage
        .set(
            "anthropic",
            json!({"type": "oauth", "access": "q15-access", "refresh": "q15-refresh", "expires": 9999999999999_f64}),
        )
        .unwrap();
    let oauth = snapshot(&registry);
    storage.remove("anthropic").unwrap();
    assert_eq!(
        json!({"before": before, "after": after, "oauth": oauth, "removed": snapshot(&registry)}),
        expected["output"]
    );
}
