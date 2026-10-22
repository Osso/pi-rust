//! Mask only nondeterministic assistant-message clocks at known capture paths.

use serde_json::Value;
use std::collections::BTreeSet;

pub fn configured_headers(model: &Value, stream: &Value) -> BTreeSet<String> {
    [model, stream]
        .into_iter()
        .filter_map(|value| value.get("headers").and_then(Value::as_object))
        .flat_map(|headers| headers.keys().map(|key| key.to_ascii_lowercase()))
        .collect()
}

pub fn normalize_message_clock(message: &mut Value) {
    if let Some(timestamp) = message.get_mut("timestamp") {
        *timestamp = Value::from(0);
    }
}

pub fn normalize_event_clocks(event: &mut Value) {
    for key in ["partial", "message", "error"] {
        if let Some(message) = event.get_mut(key) {
            normalize_message_clock(message);
        }
    }
}

pub fn normalize_capture_clocks(capture: &mut Value) {
    if let Some(events) = capture.get_mut("events").and_then(Value::as_array_mut) {
        for event in events {
            normalize_event_clocks(event);
        }
    }
    if let Some(result) = capture.get_mut("result") {
        normalize_message_clock(result);
    }
}
