//! Print-only JSON shaping from frozen `modes/json-event.ts`; subscribed session/RPC events stay untouched.
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde::Serialize;

const MESSAGE_BODY_FIELDS: &[&str] = &[
    "content",
    "details",
    "summary",
    "output",
    "providerNative",
    "imageGenerationResult",
];
const ASSISTANT_BODY_FIELDS: &[&str] = &["partial", "content", "toolCall", "message", "error"];

/// `toJsonPrintModeEvent`: omit only the listed top-level body fields, retaining other keys and order.
/// Invalid JavaScript runtime input types are outside the typed session-event contract.
pub fn to_json_print_mode_event<T: Serialize>(event: &T) -> JsValue {
    let mut value = js_json::to_value(event).expect("session events must serialize");
    let kind = value.get("type").and_then(JsValue::as_string).cloned();
    match kind.as_ref() {
        Some(kind) if kind == "message_start" => {
            omit_fields(
                value.get_mut("message").expect("message_start carries message"),
                MESSAGE_BODY_FIELDS,
            );
        }
        Some(kind) if kind == "message_update" => {
            omit_fields(&mut value, &["message"]);
            omit_fields(
                value
                    .get_mut("assistantMessageEvent")
                    .expect("message_update carries assistantMessageEvent"),
                ASSISTANT_BODY_FIELDS,
            );
        }
        Some(kind) if kind == "turn_end" => omit_fields(&mut value, &["message", "toolResults"]),
        Some(kind) if kind == "agent_end" => omit_fields(&mut value, &["messages"]),
        Some(kind) if kind == "tool_execution_end" => {
            omit_fields(
                value.get_mut("result").expect("tool_execution_end carries result"),
                &["content", "details"],
            );
        }
        _ => {}
    }
    value
}

fn omit_fields(value: &mut JsValue, fields: &[&str]) {
    let object = value.as_object_mut().expect("typed event body must be an object");
    for field in fields {
        object.shift_remove(&JsString::from(*field));
    }
}

#[cfg(test)]
mod lossless_tests {
    #[test]
    fn stripped_event_keeps_raw_delta() {
        let raw = pi_js_compat::js_json::parse_lossless(
            r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"\ud800","partial":{}}}"#,
        )
        .unwrap();
        let event = super::to_json_print_mode_event(&raw);
        let encoded = crate::modes::rpc::serialize_json_line(&event).unwrap();
        assert!(encoded.contains(r#""delta":"\ud800""#));
        assert!(!encoded.contains("partial"));
    }
}
