//! Port of `packages/coding-agent/src/modes/json-event.ts`.
//!
//! Streaming wire events omit the cumulative assistant snapshot: `message_start`
//! carries the initial message, deltas build it, and `message_end` carries the
//! final authoritative message.

use serde::Serialize;
use serde_json::Value;

/// Rebuilds `message_update` wire events as `{ type, assistantMessageEvent }`
/// without the cumulative `partial` snapshot.
///
/// Accepts any serializable session event so call sites can pass the typed
/// [`crate::core::agent_session::AgentSessionEvent`] directly; the transform only
/// inspects the wire shape, like the source runtime checks. Other events are
/// returned unchanged. Serialization keeps the TypeScript key order because the
/// crate builds `serde_json` with `preserve_order`, and removing a key keeps the
/// remaining order.
pub fn to_json_event<T: Serialize>(event: &T) -> Value {
    let value = serde_json::to_value(event).expect("session events must serialize");
    if value.get("type").and_then(Value::as_str) != Some("message_update") {
        return value;
    }

    let mut assistant_message_event = value
        .get("assistantMessageEvent")
        .cloned()
        .expect("message_update events carry assistantMessageEvent");
    if let Some(object) = assistant_message_event.as_object_mut() {
        object.remove("partial");
    }
    serde_json::json!({"type": "message_update", "assistantMessageEvent": assistant_message_event})
}
