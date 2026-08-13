/// Exact system prompt used for conversation summarization.
pub const SUMMARIZATION_SYSTEM_PROMPT: &str = "You are a context summarization assistant. Your task is to read a conversation between a user and an AI assistant, then produce a structured summary following the exact format specified.\n\nDo NOT continue the conversation. Do NOT respond to any questions in the conversation. ONLY output the structured summary.";

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
