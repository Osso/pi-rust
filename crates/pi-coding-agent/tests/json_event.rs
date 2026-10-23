//! Behavioral tests for `modes/json-event.ts`.

use pi_coding_agent::modes::json_event::to_json_event;
use serde_json::{Value, json};

fn partial() -> Value {
    json!({
        "role": "assistant",
        "content": [{"type": "text", "text": "Hi"}],
        "api": "anthropic-messages",
        "provider": "anthropic",
        "model": "claude-sonnet-4-5",
        "usage": {
            "input": 1,
            "output": 2,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": 3,
            "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}
        },
        "stopReason": "stop",
        "timestamp": 123
    })
}

fn transform(value: Value) -> String {
    serde_json::to_string(&to_json_event(&value)).expect("json event must serialize")
}

#[test]
fn message_update_drops_the_cumulative_partial_snapshot() {
    let event = json!({
        "type": "message_update",
        "message": partial(),
        "assistantMessageEvent": {
            "type": "text_delta",
            "contentIndex": 0,
            "delta": "Hi",
            "partial": partial()
        }
    });

    assert_eq!(
        transform(event),
        r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hi"}}"#
    );
}

#[test]
fn message_update_keeps_the_delta_type_when_only_partial_was_present() {
    let event = json!({
        "type": "message_update",
        "message": partial(),
        "assistantMessageEvent": {"type": "start", "partial": partial()}
    });

    assert_eq!(
        transform(event),
        r#"{"type":"message_update","assistantMessageEvent":{"type":"start"}}"#
    );
}

#[test]
fn message_update_without_partial_is_returned_unchanged() {
    let event = json!({
        "type": "message_update",
        "message": partial(),
        "assistantMessageEvent": {"type": "done", "reason": "stop", "message": partial()}
    });

    assert_eq!(
        transform(event),
        format!(
            r#"{{"type":"message_update","assistantMessageEvent":{{"type":"done","reason":"stop","message":{}}}}}"#,
            serde_json::to_string(&partial()).expect("partial")
        )
    );
}

#[test]
fn other_session_events_pass_through_unchanged() {
    for event in [
        json!({"type": "agent_start"}),
        json!({"type": "queue_update", "steering": ["a"], "followUp": []}),
        json!({"type": "thinking_level_changed", "level": "high"}),
        json!({"type": "session_info_changed", "name": "x"}),
        json!({"type": "message_start", "message": partial()}),
        json!({"type": "message_end", "message": partial()}),
    ] {
        let expected = serde_json::to_string(&event).expect("fixture");
        assert_eq!(transform(event), expected);
    }
}
