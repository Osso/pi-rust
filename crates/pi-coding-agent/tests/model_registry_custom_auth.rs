use crate::shared_oracle as oracle;

use pi_ai::providers::models_data::builtin_model;
use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_registry::{ModelRegistry, ProviderConfigInput, ResolvedRequestAuth},
};
use serde_json::json;

// Frozen model-registry.ts:877–899 validates before applyProviderConfig (:836).
// A model's baseUrl does not satisfy the required provider configuration field.
#[tokio::test]
async fn custom_provider_key_propagates_only_after_frozen_registration_validation() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    for has_base_url in [false, true] {
        let expected = source.request(&json!({"operation":"register_custom_auth","baseUrl":has_base_url}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let registry = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap();
        let mut model = builtin_model("anthropic", "claude-opus-4-8").unwrap();
        model.provider = "ordered-custom".into();
        let error = registry
            .register_provider(
                "ordered-custom",
                ProviderConfigInput {
                    api: Some(model.api.clone()),
                    api_key: Some("private-fixture-key".into()),
                    base_url: has_base_url.then(|| model.base_url.clone()),
                    models: Some(vec![model.clone().into()]),
                    ..Default::default()
                },
            )
            .err()
            .map(|error| error.error_or_string_message().unwrap().to_utf8().unwrap());
        let actual = match registry.get_api_key_and_headers(&model).await.unwrap() {
            ResolvedRequestAuth::Ok { api_key, headers, env } => {
                assert!(headers.is_none());
                assert!(env.is_none());
                let mut value = json!({"ok":true});
                if let Some(key) = api_key {
                    value["apiKey"] = json!(key.to_utf8().unwrap());
                }
                value
            }
            ResolvedRequestAuth::Err { error } => json!({"ok":false,"error":error.to_utf8().unwrap()}),
        };
        assert_eq!(
            json!({"error":error,"configured":registry.has_configured_auth(&model).unwrap(),"auth":actual}),
            expected["output"]
        );
        if has_base_url {
            assert_eq!(actual["apiKey"], json!("private-fixture-key"));
        }
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
