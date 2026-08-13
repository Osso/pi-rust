/// Mutable compaction thresholds and retention settings.
#[derive(Debug)]
pub struct CompactionSettings {
    pub enabled: bool,
    pub reserve_tokens: f64,
    pub keep_recent_tokens: f64,
}

/// Return whether context usage exceeds the configured compaction threshold.
pub fn should_compact(context_tokens: f64, context_window: f64, settings: &CompactionSettings) -> bool {
    if !settings.enabled {
        return false;
    }

    context_tokens > context_window - settings.reserve_tokens
}

pub mod utils;
