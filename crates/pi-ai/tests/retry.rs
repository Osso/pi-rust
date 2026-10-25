use pi_ai::types::{AssistantMessage, StopReason};
use pi_ai::utils::retry::is_retryable_assistant_error;

fn message(stop_reason: StopReason, error_message: Option<&str>) -> AssistantMessage {
    AssistantMessage {
        stop_reason,
        error_message: error_message.map(Into::into),
        ..AssistantMessage::empty("openai-completions", "test", "model", 1.0)
    }
}

fn error(error_message: &str) -> AssistantMessage {
    message(StopReason::Error, Some(error_message))
}

#[test]
fn classifies_provider_retry_errors() {
    assert!(!is_retryable_assistant_error(&error(
        "Incomplete response returned, reason: max_output_tokens"
    )));
    assert!(is_retryable_assistant_error(&error(
        "Incomplete response returned, reason: unknown"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "Incomplete response returned, reason: content_filter"
    )));
    assert!(is_retryable_assistant_error(&error(
        "OpenAI Responses stream ended before a terminal response event"
    )));
    assert!(is_retryable_assistant_error(&error(
        "The server had an error while processing your request. Sorry about that!"
    )));
    assert!(is_retryable_assistant_error(&error(
        "An error occurred while processing your request. You can retry your request, or contact us through our help center at help.openai.com if the error persists. Please include the request ID req_******** in your message."
    )));
    assert!(is_retryable_assistant_error(&error(
        "{\"message\":\"The system encountered an unexpected error during processing. Try your request again.\"}"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "ResourceExhausted: Worker local total request limit reached (288/48)"
    )));
    assert!(is_retryable_assistant_error(&error(
        "The socket connection was closed unexpectedly. For more information, pass `verbose: true` in the second argument to fetch()"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "Error: exceeded request buffer limit while retrying upstream"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "The pending stream has been canceled (caused by: getaddrinfo ENOTFOUND bedrock-runtime.us-east-1.amazonaws.com)"
    )));
    assert!(!is_retryable_assistant_error(&error(
        "connect ENOTFOUND api.example.com"
    )));
    assert!(!is_retryable_assistant_error(&error("EAI_AGAIN api.example.com")));
    assert!(!is_retryable_assistant_error(&error(
        "getaddrinfo failed for api.example.com"
    )));
    assert!(is_retryable_assistant_error(&error(
        "OpenAI Responses stream ended before a terminal response event"
    )));
    assert!(!is_retryable_assistant_error(&error("429 quota exceeded")));
    assert!(is_retryable_assistant_error(&error("overloaded_error")));
    assert!(is_retryable_assistant_error(&error("524 status code (no body)")));
    assert!(!is_retryable_assistant_error(&message(StopReason::Stop, None)));
    assert!(!is_retryable_assistant_error(&message(StopReason::Error, None)));
    assert!(!is_retryable_assistant_error(&error("insufficient_quota")));
    assert!(!is_retryable_assistant_error(&error("Monthly usage limit reached")));
    assert!(!is_retryable_assistant_error(&error("GoUsageLimitError")));
}

#[test]
fn frozen_exact_transport_and_access_errors() {
    for text in [
        "WebSocket connection to 'wss://example.com' failed: Expected 101 status code",
        "Codex error: Unable to verify Daybreak Blue access. Please try again.",
    ] {
        assert!(is_retryable_assistant_error(&error(text)), "{text}");
    }
    let mut blocked = error("This content was flagged for possible cybersecurity risk. 503");
    blocked.api = "openai-codex-responses".into();
    assert!(!is_retryable_assistant_error(&blocked));
}

#[test]
fn frozen_codex_cybersecurity_wins_over_transient_suffix() {
    for text in [
        "This content was flagged for possible cybersecurity risk. 503",
        "Codex error: This content was flagged for possible cybersecurity risk. timeout",
    ] {
        let mut blocked = error(text);
        blocked.api = "openai-codex-responses".into();
        assert!(!is_retryable_assistant_error(&blocked), "{text}");
    }
}

#[test]
fn frozen_regex_uses_javascript_characters() {
    for text in ["rate😀limit", "rate\u{2028}limit", "ſerver error", "socKet hang up"] {
        assert!(!is_retryable_assistant_error(&error(text)), "{text}");
    }
}
