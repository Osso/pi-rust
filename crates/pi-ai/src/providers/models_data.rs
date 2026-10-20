//! Port of `packages/ai/src/providers/*.models.ts`.
//!
//! Every generated catalog module is the same two statements
//! (`flattenModelCatalog(provider, data/<provider>.json)`), so one Rust module
//! embeds all catalogs instead of 40 identical files.

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
    Some(flatten_catalog(json))
}

/// `MODELS[provider][modelId]`.
pub fn builtin_model(provider: &str, model_id: &str) -> Option<Model> {
    provider_models(provider)?
        .into_iter()
        .find(|model| model.id == model_id)
}

/// `getBuiltinModelDataGeneratedAt()`.
pub fn model_data_generated_at() -> Option<f64> {
    let manifest: Value = serde_json::from_str(MODEL_DATA_MANIFEST).ok()?;
    let generated_at = manifest.get("generatedAt")?.as_str()?;
    let millis = pi_js_compat::date::date_parse(generated_at);
    (!millis.is_nan()).then_some(millis)
}

/// `flattenModelCatalog`: group values in file order, later duplicates replace
/// earlier ones without moving their position.
fn flatten_catalog(json: &str) -> Vec<Model> {
    let groups: serde_json::Map<String, Value> =
        serde_json::from_str(json).expect("embedded provider catalogs must be valid JSON");
    let mut catalog: Vec<(String, Model)> = Vec::new();
    for group in groups.into_values() {
        let Value::Object(models) = group else { continue };
        for (id, value) in models {
            let model: Model =
                serde_json::from_value(value).expect("embedded provider catalogs must match the model contract");
            match catalog.iter_mut().find(|(existing, _)| *existing == id) {
                Some(entry) => entry.1 = model,
                None => catalog.push((id, model)),
            }
        }
    }
    catalog.into_iter().map(|(_, model)| model).collect()
}
