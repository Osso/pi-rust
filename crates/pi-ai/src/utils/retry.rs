use std::sync::OnceLock;

use regress::Regex;

use crate::types::{AssistantMessage, StopReason};

const NON_RETRYABLE_PROVIDER_ERROR_PATTERNS: [&str; 10] = [
    "GoUsageLimitError",
    "FreeUsageLimitError",
    "Monthly usage limit reached",
    "available balance",
    "insufficient_quota",
    "out of budget",
    "quota exceeded",
    "billing",
    "Incomplete response returned, reason: max_output_tokens",
    "Incomplete response returned, reason: content_filter",
];

const RETRYABLE_PROVIDER_ERROR_PATTERNS: [&str; 41] = [
    "overloaded",
    "rate.?limit",
    "too many requests",
    "429",
    "500",
    "502",
    "503",
    "504",
    "524",
    "service.?unavailable",
    "server.?error",
    "server had an error",
    "internal.?error",
    "provider.?returned.?error",
    "network.?error",
    "connection.?error",
    "connection.?refused",
    "connection.?lost",
    "other side closed",
    "fetch failed",
    "upstream.?connect",
    "reset before headers",
    "socket hang up",
    "socket connection was closed",
    "closed unexpectedly",
    "timed? out",
    "timeout",
    "terminated",
    "websocket.?closed",
    "websocket.?error",
    r"^(?:Error:\s*)?WebSocket connection to 'wss://[^'\s]+' failed: Expected 101 status\s+code$",
    "Incomplete response returned",
    "ended without",
    "stream ended before message_stop",
    "stream ended before a terminal response event",
    "http2 request did not get a response",
    "retry delay",
    r"^Codex error: Unable to verify Daybreak Blue access\. Please try again\.(?:\nOpenAI request ID: [^\r\n]+)?$",
    "you can retry your request",
    "try your request again",
    "please retry your request",
];

fn non_retryable_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| build_provider_error_pattern(&NON_RETRYABLE_PROVIDER_ERROR_PATTERNS))
}

fn retryable_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| build_provider_error_pattern(&RETRYABLE_PROVIDER_ERROR_PATTERNS))
}

fn build_provider_error_pattern(patterns: &[&str]) -> Regex {
    Regex::with_flags(&patterns.join("|").to_ascii_lowercase(), "").expect("provider error patterns are valid regexes")
}

/// Identifies the canonical Codex cybersecurity restriction, including suffixes.
pub fn is_codex_cybersecurity_restriction(message: &AssistantMessage) -> bool {
    const RESTRICTION: &str = "This content was flagged for possible cybersecurity risk.";
    if message.stop_reason != StopReason::Error || message.api != "openai-codex-responses" {
        return false;
    }
    let Some(error_message) = &message.error_message else {
        return false;
    };
    error_message.starts_with(RESTRICTION) || error_message.starts_with(format!("Codex error: {RESTRICTION}"))
}

/// Classifies whether a failed assistant message looks like a transient provider
/// or transport error.
pub fn is_retryable_assistant_error(message: &AssistantMessage) -> bool {
    if message.stop_reason != StopReason::Error {
        return false;
    }
    let Some(error_message) = &message.error_message else {
        return false;
    };
    if error_message.is_empty() || is_codex_cybersecurity_restriction(message) {
        return false;
    }
    let error_message = super::error_body::fold_ascii_error_text(error_message);
    if non_retryable_pattern()
        .find_from_ucs2(error_message.units(), 0)
        .next()
        .is_some()
    {
        return false;
    }
    retryable_pattern()
        .find_from_ucs2(error_message.units(), 0)
        .next()
        .is_some()
}
