//! Ported from `coding-agent/src/modes/interactive/model-search.ts`.

/// One model row the search matches against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSearchItem {
    pub id: String,
    pub provider: String,
    pub name: Option<String>,
}

/// Candidate strings a model is fuzzy-matched against, in source order.
pub fn get_model_search_candidates(item: &ModelSearchItem) -> Vec<String> {
    let mut candidates = vec![
        item.id.clone(),
        format!("{}/{}", item.provider, item.id),
        format!("{} {}", item.provider, item.id),
    ];
    if let Some(name) = &item.name
        && !name.is_empty()
    {
        candidates.push(name.clone());
    }
    candidates
}
