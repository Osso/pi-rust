//! Path helpers shared by the repository and search backend.

/// Returns the parent of `path` with the source backend's separator handling.
pub(crate) fn get_parent_path(path: &str) -> String {
    let normalized = path.trim_end_matches(['/', '\\']);
    match normalized.rfind(['/', '\\']) {
        None => ".".to_owned(),
        Some(0) => normalized[..1].to_owned(),
        Some(index) => normalized[..index].to_owned(),
    }
}
