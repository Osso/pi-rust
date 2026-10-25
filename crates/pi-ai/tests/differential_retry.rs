use crate::shared_json_compare as json_compare;
use crate::shared_oracle as oracle;

use pi_ai::types::AssistantMessage;
use pi_ai::utils::retry::{is_codex_cybersecurity_restriction, is_retryable_assistant_error};
use serde_json::{Value, json};

fn response(stop_reason: &str, error_message: Option<&str>, _text: &str) -> Value {
    let mut value = serde_json::to_value(AssistantMessage::empty("openai-completions", "test", "model", 1.0)).unwrap();
    value["stopReason"] = json!(stop_reason);
    if let Some(error) = error_message {
        value["errorMessage"] = json!(error);
    }
    value
}

fn classification_messages() -> Vec<Value> {
    let mut messages: Vec<Value> = [
        "Incomplete response returned, reason: max_output_tokens",
        "Incomplete response returned, reason: unknown",
        "Incomplete response returned, reason: content_filter",
        "OpenAI Responses stream ended before a terminal response event",
        "The server had an error while processing your request. Sorry about that!",
        "An error occurred while processing your request. You can retry your request, or contact us through our help center at help.openai.com if the error persists. Please include the request ID req_******** in your message.",
        "{\"message\":\"The system encountered an unexpected error during processing. Try your request again.\"}",
        "ResourceExhausted: Worker local total request limit reached (288/48)",
        "The socket connection was closed unexpectedly. For more information, pass `verbose: true` in the second argument to fetch()",
        "Error: exceeded request buffer limit while retrying upstream",
        "The pending stream has been canceled (caused by: getaddrinfo ENOTFOUND bedrock-runtime.us-east-1.amazonaws.com)",
        "connect ENOTFOUND api.example.com",
        "EAI_AGAIN api.example.com",
        "getaddrinfo failed for api.example.com",
        "429 quota exceeded",
        "overloaded_error",
        "524 status code (no body)",
        "insufficient_quota",
        "Monthly usage limit reached",
        "GoUsageLimitError",
        "FreeUsageLimitError",
        "available balance",
        "out of budget",
        "quota exceeded",
        "billing",
        "rate limit",
        "too many requests",
        "service unavailable",
        "connection refused",
        "other side closed",
        "fetch failed",
        "upstream connect error",
        "reset before headers",
        "socket hang up",
        "closed unexpectedly",
        "timed out",
        "timeout",
        "terminated",
        "websocket closed",
        "websocket error",
        "ended without",
        "stream ended before message_stop",
        "http2 request did not get a response",
        "retry delay",
        "please retry your request",
    ]
    .into_iter()
    .map(|message| response("error", Some(message), ""))
    .collect();
    for text in [
        "429",
        "500",
        "502",
        "503",
        "504",
        "server error",
        "internal error",
        "provider returned error",
        "network error",
        "connection error",
        "connection lost",
        "time out",
        "Incomplete response returned",
        "stream ended before a terminal response event",
    ] {
        messages.push(response("error", Some(text), ""));
        messages.push(response(
            "error",
            Some(&format!("prefix {} suffix", text.to_uppercase())),
            "",
        ));
    }
    for text in [
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
    ] {
        messages.push(response(
            "error",
            Some(&format!("503 {} timeout", text.to_uppercase())),
            "",
        ));
    }
    for stem in [
        "rate|limit",
        "service|unavailable",
        "server|error",
        "internal|error",
        "provider|returned|error",
        "network|error",
        "connection|error",
        "connection|refused",
        "connection|lost",
        "upstream|connect",
        "websocket|closed",
        "websocket|error",
    ] {
        for separator in [
            "", " ", "-", "_", "X", "é", "😀", "\n", "\r", "\u{2028}", "\u{2029}", "  ",
        ] {
            messages.push(response("error", Some(&stem.replace('|', separator)), ""));
        }
    }
    let websocket = "WebSocket connection to 'wss://example.com/path' failed: Expected 101 status code";
    let daybreak = "Codex error: Unable to verify Daybreak Blue access. Please try again.";
    for text in [websocket, daybreak] {
        for suffix in ["", "x", "\n", "\r", "\r\n", "\u{2028}", "\u{2029}", "\n\n"] {
            messages.push(response("error", Some(&format!("{text}{suffix}")), ""));
        }
        messages.push(response("error", Some(&format!("prefix {text}")), ""));
        messages.push(response("error", Some(&text.to_uppercase()), ""));
    }
    for text in [
        "Error: WebSocket connection to 'wss://x' failed: Expected 101 status code",
        "Error:WebSocket connection to 'wss://x' failed: Expected 101 status code",
        "Error:  WebSocket connection to 'wss://x' failed: Expected 101 status  code",
        "WebSocket connection to 'wss://x' failed: Expected 101 statuscode",
        "WebSocket connection to 'wss://a'b' failed: Expected 101 status code",
        "providerreturned error",
        "provider returnederror",
        "Codex error: Unable to verify Daybreak Blue access. Please try again.\r\nOpenAI request ID: req_1",
        "Error:\tWebSocket connection to 'wss://x' failed: Expected 101 status\ncode",
        "WebSocket connection to 'ws://x' failed: Expected 101 status code",
        "WebSocket connection to 'wss://' failed: Expected 101 status code",
        "WebSocket connection to 'wss://a b' failed: Expected 101 status code",
        "WebSocket connection to 'wss://a\u{feff}b' failed: Expected 101 status code",
        "WebSocket connection to 'wss://a\u{85}b' failed: Expected 101 status code",
        "WebSocket connection to 'wss://x' failed: Expected 101 status\u{feff}code",
        "WebSocket connection to 'wss://x' failed: Expected 101 status\u{85}code",
        "Codex error: Unable to verify Daybreak Blue accessX Please try again.",
        "Codex error: Unable to verify Daybreak Blue access. Please try againX",
        "Codex error: Unable to verify Daybreak Blue access. Please try again.\nOpenAI request ID: req_1",
        "Codex error: Unable to verify Daybreak Blue access. Please try again.\nOpenAI request ID: ",
        "Codex error: Unable to verify Daybreak Blue access. Please try again.\nOpenAI request ID: req_1\nextra",
        "ſerver error",
        "socKet hang up",
        "503 bılling",
        "unrecognized",
        "",
    ] {
        messages.push(response("error", Some(text), ""));
    }
    for stop in ["stop", "length", "toolUse", "aborted", "error"] {
        for text in [None, Some(""), Some("503")] {
            messages.push(response(stop, text, ""));
        }
        for api in ["openai-codex-responses", "openai-responses", "OPENAI-CODEX-RESPONSES"] {
            for text in [None, Some("")] {
                let mut value = response(stop, text, "");
                value["api"] = json!(api);
                messages.push(value);
            }
            for text in [
                "This content was flagged for possible cybersecurity risk.",
                "This content was flagged for possible cybersecurity risk. 503 request-id",
                "Codex error: This content was flagged for possible cybersecurity risk. timeout",
                "this content was flagged for possible cybersecurity risk. 503",
                "prefix This content was flagged for possible cybersecurity risk. 503",
                " This content was flagged for possible cybersecurity risk. 503",
                "Codex error:  This content was flagged for possible cybersecurity risk. 503",
                "This content was flagged for possible cybersecurity riskX 503",
            ] {
                let mut value = response(stop, Some(text), "");
                value["api"] = json!(api);
                messages.push(value);
            }
        }
    }
    for whitespace in [
        '\t', '\n', '\u{b}', '\u{c}', '\r', ' ', '\u{a0}', '\u{1680}', '\u{2000}', '\u{200a}', '\u{2028}', '\u{2029}',
        '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}', '\u{85}', '\u{180e}', '\u{200b}',
    ] {
        for text in [
            format!("Error:{whitespace}WebSocket connection to 'wss://x' failed: Expected 101 status code"),
            format!("WebSocket connection to 'wss://x' failed: Expected 101 status{whitespace}code"),
            format!("WebSocket connection to 'wss://a{whitespace}b' failed: Expected 101 status code"),
        ] {
            messages.push(response("error", Some(&text), ""));
        }
    }
    for api in ["openai-codex-responses", "openai-responses", "OPENAI-CODEX-RESPONSES"] {
        for text in [websocket, daybreak] {
            let mut value = response("error", Some(text), "");
            value["api"] = json!(api);
            messages.push(value);
        }
    }
    for request_id in ["req_1", "req_1\n", "req_1\r", "req_1\r\n", "😀", "req\u{2028}id", " "] {
        let text = format!("{daybreak}\nOpenAI request ID: {request_id}");
        messages.push(response("error", Some(&text), ""));
    }
    eprintln!("{} source classifier requests (both exports)", messages.len());
    messages
}

#[test]
fn frozen_retry_classifiers_match_source() {
    let mut oracle = oracle::TsOracle::spawn("retry-oracle.ts", "packages/ai/src/utils/retry.ts");
    for (id, message) in classification_messages().into_iter().enumerate() {
        let native: AssistantMessage = serde_json::from_value(message.clone()).unwrap();
        let expected = oracle.request(&json!({"id": id, "message": message}));
        let observed = json!({"id": id, "ok": true, "output": {"retryable": is_retryable_assistant_error(&native), "cybersecurity": is_codex_cybersecurity_restriction(&native)}});
        json_compare::assert_json_eq_ordered(&observed, &expected, &format!("request {id}: {message}"));
    }
}
