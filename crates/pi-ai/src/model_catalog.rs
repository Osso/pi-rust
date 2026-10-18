use indexmap::IndexMap;

use crate::types::Model;

/// Grouped model data: group name to model ID to model.
pub type ModelGroups = IndexMap<String, IndexMap<String, Model>>;

/// Flattened catalog keyed by model ID.
pub type ModelCatalog = IndexMap<String, Model>;

/// Flatten grouped model data, like `Object.assign({}, ...Object.values(groups))`.
///
/// The TypeScript `provider` parameter is type-only; model IDs and providers are
/// already present in the generated model values.
pub fn flatten_model_catalog(groups: impl IntoIterator<Item = IndexMap<String, Model>>) -> ModelCatalog {
    let mut catalog = ModelCatalog::new();
    for group in groups {
        catalog.extend(group);
    }
    catalog
}
