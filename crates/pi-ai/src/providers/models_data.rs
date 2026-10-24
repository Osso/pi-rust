//! Port of `packages/ai/src/providers/*.models.ts`.
//!
//! Frozen literal records from `models.generated.ts` and its 36 imported shards.
//! JSON artifacts retain source model keys and order; no factories or supplements.

use serde_json::Value;

use crate::types::Model;

// Frozen-source include table; maintenance generator re-pin remains staged.
include!("model_catalogs_generated.rs");

/// `Object.keys(MODELS)`.
pub fn builtin_provider_ids() -> Vec<String> {
    PROVIDER_CATALOGS
        .iter()
        .map(|(provider, _)| (*provider).to_owned())
        .collect()
}

/// `Object.values(MODELS[provider])`, preserving generated insertion order.
pub fn provider_models(provider: &str) -> Option<Vec<Model>> {
    let (_, json) = PROVIDER_CATALOGS.iter().find(|(id, _)| *id == provider)?;
    Some(read_catalog(json).into_values().map(read_model).collect())
}

/// `MODELS[provider][modelId]`.
pub fn builtin_model(provider: &str, model_id: &str) -> Option<Model> {
    let (_, json) = PROVIDER_CATALOGS.iter().find(|(id, _)| *id == provider)?;
    read_catalog(json).remove(model_id).map(read_model)
}

fn read_catalog(json: &str) -> serde_json::Map<String, Value> {
    serde_json::from_str(json).expect("embedded provider catalogs must be valid literal records")
}

fn read_model(value: Value) -> Model {
    serde_json::from_value(value).expect("embedded provider catalogs must match the model contract")
}
