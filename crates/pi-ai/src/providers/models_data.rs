//! Port of `packages/ai/src/providers/*.models.ts`.
//!
//! Frozen literal records from `models.generated.ts` and its 36 imported shards.
//! JSON artifacts retain source model keys and order; no factories or supplements.

use serde_json::Value;

use crate::types::Model;

// Native include table; generator updates this glue alongside the JSON catalogs.
include!("model_catalogs_generated.rs");

const MODEL_DATA_MANIFEST: &str = include_str!("../../data/providers/.manifest.json");

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

/// Staged old-pin API: retained only for coding-agent model_runtime compilation.
/// Frozen literal provenance has no generation timestamp, so this returns None.
pub fn model_data_generated_at() -> Option<f64> {
    let manifest: Value = serde_json::from_str(MODEL_DATA_MANIFEST).ok()?;
    let generated_at = manifest.get("generatedAt")?.as_str()?;
    let millis = pi_js_compat::date::date_parse(generated_at);
    (!millis.is_nan()).then_some(millis)
}

fn read_catalog(json: &str) -> serde_json::Map<String, Value> {
    serde_json::from_str(json).expect("embedded provider catalogs must be valid literal records")
}

fn read_model(value: Value) -> Model {
    serde_json::from_value(value).expect("embedded provider catalogs must match the model contract")
}
