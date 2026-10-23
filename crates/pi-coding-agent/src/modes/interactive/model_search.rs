//! Ported from `packages/coding-agent/src/modes/interactive/model-search.ts`.

use pi_js_compat::js_json::JsString;

/// Model fields used for fuzzy search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelSearchItem {
    pub id: String,
    pub provider: String,
    pub name: Option<JsString>,
}

/// Candidate strings a model is fuzzy-matched against.
pub fn get_model_search_candidates(item: &ModelSearchItem) -> Vec<JsString> {
    let mut candidates = vec![
        item.id.clone().into(),
        format!("{}/{}", item.provider, item.id).into(),
        format!("{} {}", item.provider, item.id).into(),
    ];
    // The source's `if (name)` drops an empty display name.
    if let Some(name) = &item.name
        && !name.is_empty()
    {
        candidates.push(name.clone());
    }
    candidates
}
