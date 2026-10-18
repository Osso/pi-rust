//! Ported from `packages/coding-agent/src/modes/interactive/model-search.ts`.

/// Model fields used for fuzzy search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelSearchItem {
    pub id: String,
    pub provider: String,
    pub name: Option<String>,
}

/// Candidate strings a model is fuzzy-matched against.
pub fn get_model_search_candidates(item: &ModelSearchItem) -> Vec<String> {
    let mut candidates = vec![
        item.id.clone(),
        format!("{}/{}", item.provider, item.id),
        format!("{} {}", item.provider, item.id),
    ];
    if let Some(name) = &item.name {
        candidates.push(name.clone());
    }
    candidates
}
