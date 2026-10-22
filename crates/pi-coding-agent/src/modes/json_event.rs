//! Port of `packages/coding-agent/src/modes/json-event.ts`.
//!
//! Streaming wire events omit the cumulative assistant snapshot: `message_start`
//! carries the initial message, deltas build it, and `message_end` carries the
//! final authoritative message.

use pi_js_compat::js_json::{self, JsValue};
use serde::Serialize;

/// Rebuilds `message_update` wire events as `{ type, assistantMessageEvent }`
/// without the cumulative `partial` snapshot.
///
/// Accepts any serializable session event so call sites can pass the typed
/// [`crate::core::agent_session::AgentSessionEvent`] directly; the transform only
/// inspects the wire shape, like the source runtime checks. Other events are
/// returned unchanged. Serialization keeps the TypeScript key order because the
/// crate builds `serde_json` with `preserve_order`, and removing a key keeps the
/// remaining order.
pub fn to_json_event<T: Serialize>(event: &T) -> JsValue {
    let value = js_json::to_value(event).expect("session events must serialize");
    if value.get("type").and_then(JsValue::as_string) != Some(&"message_update".into()) {
        return value;
    }

    let mut assistant_message_event = value
        .get("assistantMessageEvent")
        .cloned()
        .expect("message_update events carry assistantMessageEvent");
    if let Some(object) = assistant_message_event.as_object_mut() {
        object.shift_remove(&pi_js_compat::js_json::JsString::from("partial"));
    }
    pi_js_compat::js_json!({"type": "message_update", "assistantMessageEvent": assistant_message_event})
}

#[cfg(test)]
mod lossless_tests {
    #[test]
    fn stripped_event_keeps_raw_delta() {
        let raw = pi_js_compat::js_json::parse_lossless(
            r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"\ud800","partial":{}}}"#,
        )
        .unwrap();
        let event = super::to_json_event(&raw);
        let encoded = crate::modes::rpc::serialize_json_line(&event).unwrap();
        assert!(encoded.contains(r#""delta":"\ud800""#));
        assert!(!encoded.contains("partial"));
    }
}
