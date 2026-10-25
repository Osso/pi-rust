//! Frozen all.ts inventory and chat/image collection registration boundaries.
use pi_ai::images_models::ImagesModels;
use pi_ai::models::{CreateModelsOptions, Models, MutableModels};
use pi_ai::providers::all::{builtin_images_models, builtin_images_providers, builtin_models, builtin_providers};
use serde_json::json;

use crate::shared_oracle as oracle;

#[test]
fn builtin_factory_ids_and_order_match_frozen_all_module() {
    let mut ts = oracle::TsOracle::spawn("provider-inventory-oracle.ts", "packages/ai/src/providers/all.ts");
    let response = ts.request(&json!({"id": 1}));
    assert_eq!(response["ok"], true);
    let providers = builtin_providers();
    let ids = providers.iter().map(|provider| provider.id()).collect::<Vec<_>>();
    assert_eq!(json!(ids), response["output"]);
}

#[test]
fn builtin_collections_register_all_models_and_keep_fresh_instances_independent() {
    let mut ts = oracle::TsOracle::spawn("provider-inventory-oracle.ts", "packages/ai/src/providers/all.ts");
    let response = ts.request(&json!({"id": 2, "op": "collections"}));
    assert_eq!(response["ok"], true, "{response}");
    let chat = builtin_models(CreateModelsOptions::default());
    let images = builtin_images_models(CreateModelsOptions::default());
    let other = builtin_models(CreateModelsOptions::default());
    other.clear_providers();
    assert_eq!(
        json!({
            "chat": {
                "providers": chat.get_providers().iter().map(|p| p.id()).collect::<Vec<_>>(),
                "models": chat.get_models(None),
            },
            "images": {
                "providers": builtin_images_providers().iter().map(|p| p.id()).collect::<Vec<_>>(),
                "models": images.get_models(None),
            },
        }),
        response["output"],
    );
}
