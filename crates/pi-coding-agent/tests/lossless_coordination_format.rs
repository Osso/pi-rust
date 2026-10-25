use crate::shared_oracle as oracle;

use pi_coding_agent::core::runtime_coordination_format::format_runtime_mailbox_prompt;
use pi_coding_agent::core::session_control_db::RuntimeMailboxMessage;
use pi_js_compat::js_json::{self, JsString, JsValue};

fn raw_mailbox() -> JsValue {
    js_json::parse_lossless(r#"{"id":1,"recipient":{"sessionId":"s\ud800","agentId":null},"sender":{"sessionId":"s\ud800","agentId":"a\udfff"},"senderHost":"h\ud800","kind":"message","body":"  \ud800  ","fileRefs":[{"path":"p\ud800","label":"l\udfff"}],"storeRef":{"messageId":"m","sessionPath":"/tmp/source.jsonl"},"status":"claimed","createdAt":"2026-10-21T00:00:00Z","updatedAt":"2026-10-21T00:00:00Z"}"#).unwrap()
}

fn source_request(message: JsValue) -> JsValue {
    pi_js_compat::js_json!({
        "id": 1,
        "operation": "format",
        "cases": [{
            "name": "raw",
            "recipientSessionId": JsString::from_utf16(vec![0x73, 0xd800]),
            "message": message,
        }],
    })
}

#[test]
fn mailbox_host_body_and_file_reference_units_match_frozen() {
    let raw = raw_mailbox();
    let mut source = oracle::TsOracle::spawn(
        "runtime-coordination-format-oracle.ts",
        "packages/coding-agent/src/core/runtime-coordination-format.ts",
    );
    let expected = source.request(&source_request(raw.clone()));
    assert_eq!(expected["ok"], JsValue::Bool(true));
    let message: RuntimeMailboxMessage = js_json::from_value(&raw).unwrap();
    let actual = format_runtime_mailbox_prompt(&message, JsString::from_utf16(vec![0x73, 0xd800])).unwrap();
    assert_eq!(expected["output"]["results"][0]["prompt"], JsValue::String(actual));
}

#[test]
fn invalid_file_references_surface_the_frozen_formatting_error() {
    let mut source = oracle::TsOracle::spawn(
        "runtime-coordination-format-oracle.ts",
        "packages/coding-agent/src/core/runtime-coordination-format.ts",
    );
    for file_refs in ["{}", "[null]"] {
        let mut raw = raw_mailbox();
        let JsValue::Object(fields) = &mut raw else {
            unreachable!()
        };
        fields.insert("fileRefs".into(), js_json::parse_lossless(file_refs).unwrap());
        let expected = source.request(&source_request(raw.clone()));
        assert_eq!(expected["ok"], JsValue::Bool(false));
        let message: RuntimeMailboxMessage = js_json::from_value(&raw).unwrap();
        let error = format_runtime_mailbox_prompt(&message, "recipient").unwrap_err();
        assert_eq!(expected["error"]["message"], JsValue::String(error));
    }
}
