use std::sync::OnceLock;

use regex::Regex;

use crate::types::{AssistantMessage, StopReason};

/// Patterns matching context-overflow error messages from different providers.
const OVERFLOW_PATTERNS: [&str; 25] = [
    r"prompt is too long",
    r"request_too_large",
    r"exceeded request buffer limit while retrying upstream",
    r"input is too long for requested model",
    r"exceeds the context window",
    r"exceeds (?:the )?(?:model'?s )?maximum context length(?: of [\d,]+ tokens?|\s*\([\d,]+\))",
    r"input token count.*exceeds the maximum",
    r"maximum prompt length is \d+",
    r"reduce the length of the messages",
    r"maximum context length is \d+ tokens",
    r"exceeds (?:the )?maximum allowed input length of [\d,]+ tokens?",
    r"input \(\d+ tokens\) is longer than the model'?s context length \(\d+ tokens\)",
    r"exceeds the limit of \d+",
    r"exceeds the available context size",
    r"greater than the context length",
    r"context window exceeds limit",
    r"exceeded model token limit",
    r"too large for model with \d+ maximum context length",
    r"prompt has [\d,]+ tokens?, but the configured context size is [\d,]+ tokens?",
    r"model_context_window_exceeded",
    r"prompt too long; exceeded (?:max )?context length",
    r"range of input length should be",
    r"context[_ ]length[_ ]exceeded",
    r"too many tokens",
    r"token limit exceeded",
];

/// Non-overflow errors that also match an overflow pattern.
const NON_OVERFLOW_PATTERNS: [&str; 3] = [
    r"^(Throttling error|Service unavailable):",
    r"rate limit",
    r"too many requests",
];

fn overflow_regexes() -> &'static Vec<regress::Regex> {
    static PATTERNS: OnceLock<Vec<regress::Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let mut patterns: Vec<regress::Regex> = OVERFLOW_PATTERNS.iter().map(|pattern| compile(pattern)).collect();
        patterns.push(compile(r"^4(?:00|13)\s*(?:status code)?\s*\(no body\)"));
        patterns
    })
}

fn non_overflow_regexes() -> &'static Vec<regress::Regex> {
    static PATTERNS: OnceLock<Vec<regress::Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| NON_OVERFLOW_PATTERNS.iter().map(|pattern| compile(pattern)).collect())
}

fn compile(pattern: &str) -> regress::Regex {
    regress::Regex::with_flags(&pattern.to_ascii_lowercase(), "").expect("overflow patterns are valid regexes")
}

/// Check if an assistant message represents a context overflow error.
pub fn is_context_overflow(message: &AssistantMessage, context_window: Option<f64>) -> bool {
    if let (StopReason::Error, Some(error_message)) = (message.stop_reason, &message.error_message) {
        let error_message = super::error_body::fold_ascii_error_text(error_message);
        let is_non_overflow = non_overflow_regexes()
            .iter()
            .any(|pattern| pattern.find_from_ucs2(error_message.units(), 0).next().is_some());
        if !is_non_overflow
            && overflow_regexes()
                .iter()
                .any(|pattern| pattern.find_from_ucs2(error_message.units(), 0).next().is_some())
        {
            return true;
        }
    }

    let Some(context_window) = context_window.filter(|window| *window > 0.0) else {
        return false;
    };
    let input_tokens = message.usage.input + message.usage.cache_read;

    if message.stop_reason == StopReason::Stop && input_tokens > context_window {
        return true;
    }

    message.stop_reason == StopReason::Length && message.usage.output == 0.0 && input_tokens >= context_window * 0.99
}

/// Whether a length stop ended below the caller or model's intended output limit.
pub fn is_recoverable_length(message: &AssistantMessage, desired_max_output: f64) -> bool {
    message.stop_reason == StopReason::Length && desired_max_output > 0.0 && message.usage.output < desired_max_output
}

/// The overflow patterns, exposed for testing.
pub fn get_overflow_patterns() -> Vec<Regex> {
    OVERFLOW_PATTERNS
        .iter()
        .chain([&r"^4(?:00|13)\s*(?:status code)?\s*\(no body\)"])
        .map(|pattern| Regex::new(&format!("(?i){pattern}")).expect("overflow patterns are valid regexes"))
        .collect()
}
