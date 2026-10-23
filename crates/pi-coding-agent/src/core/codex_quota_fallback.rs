//! Frozen Codex quota fallback helpers.
use pi_ai::{AssistantMessage, StopReason};
use pi_js_compat::js_regex::JsRegex;
use std::sync::LazyLock;

static QUOTA_EXHAUSTION_PATTERN: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::with_flags("GoUsageLimitError|FreeUsageLimitError|usage limit|available balance|insufficient_quota|out of budget|quota exceeded|billing (?:limit|quota|exhausted)", "i").expect("frozen quota regex")
});

pub const CODEX_FALLBACK_PROVIDERS: [(&str, [&str; 2]); 3] = [
    ("openai-codex", ["openai-codex-gc", "openai-codex-team"]),
    ("openai-codex-gc", ["openai-codex", "openai-codex-team"]),
    ("openai-codex-team", ["openai-codex", "openai-codex-gc"]),
];

pub fn is_quota_exhaustion_error(message: &AssistantMessage) -> bool {
    message.stop_reason == StopReason::Error
        && message
            .error_message
            .as_ref()
            .is_some_and(|error| !error.is_empty() && QUOTA_EXHAUSTION_PATTERN.is_match(error))
}
