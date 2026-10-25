use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::InputSource;
use pi_coding_agent::core::agent_session::QueuedMessagePreview;
use pi_coding_agent::modes::interactive::coordination_visibility::{is_coordination_message, is_coordination_preview};
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde_json::json;

#[test]
fn coordination_provenance_and_envelope_shapes_match_frozen_typescript() {
    let bare = "From:\n- agent: peer\n\nMessage:\nbody";
    let hosted =
        "From:\n- host: aso (reply with toHost and toSessionId)\n- session: abc\n- agent: main\n\nMessage:\nbody";
    let session_only = "From:\n- session: abc\n- agent: main\n\nMessage:\nbody";
    let variants = [
        (bare, Some(InputSource::Extension), None),
        (hosted, Some(InputSource::Extension), None),
        (session_only, Some(InputSource::Extension), None),
        (bare, Some(InputSource::Interactive), None),
        (bare, Some(InputSource::Rpc), None),
        (bare, None, None),
        ("ordinary", None, Some("shared_channel")),
        ("ordinary", Some(InputSource::Extension), Some("other")),
        (
            " From:\n- agent: peer\n\nMessage:\nbody",
            Some(InputSource::Extension),
            None,
        ),
        (
            "From:\n- host: aso\n- agent: peer\n\nMessage:\nbody",
            Some(InputSource::Extension),
            None,
        ),
        ("From:\n- agent: \n\nMessage:\nbody", Some(InputSource::Extension), None),
        (
            "From:\r\n- agent: peer\r\n\r\nMessage:\r\nbody",
            Some(InputSource::Extension),
            None,
        ),
    ];
    let previews: Vec<_> = variants
        .into_iter()
        .map(|(text, input_source, custom_type)| QueuedMessagePreview {
            text: text.into(),
            input_source,
            custom_type: custom_type.map(JsString::from),
        })
        .collect();
    let messages_json = json!([
        {"role":"user","content":hosted,"inputSource":"extension","timestamp":0},
        {"role":"user","content":[{"type":"text","text":"From:\n- agent:"},{"type":"image","data":"AA==","mimeType":"image/png"},{"type":"text","text":" peer\n\nMessage:\nbody"}],"inputSource":"extension","timestamp":0},
        {"role":"user","content":bare,"inputSource":"interactive","timestamp":0},
        {"role":"user","content":bare,"timestamp":0},
        {"role":"custom","customType":"shared_channel","content":"ordinary","display":true,"timestamp":0},
        {"role":"custom","customType":"other","content":bare,"display":true,"timestamp":0},
        {"role":"bashExecution","customType":"shared_channel","command":"ordinary","timestamp":0}
    ]);
    let messages: Vec<AgentMessage> = js_json::from_value(&JsValue::from(messages_json.clone())).unwrap();
    let actual = json!({
        "previews":previews.iter().map(is_coordination_preview).collect::<Vec<_>>(),
        "messages":messages.iter().map(is_coordination_message).collect::<Vec<_>>(),
    });
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let request = json!({
        "id":"coordination", "module":"modes/interactive/coordination-visibility.ts", "operation":"coordinationVisibility",
        "previews":previews,"messages":messages_json,
    });
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(actual, expected["output"]);
}

#[test]
fn raw_envelope_units_match_without_projection() {
    let raw = JsString::from("From:\n- agent: ") + JsString::from_utf16(vec![0xd800]) + "\n\nMessage:\n";
    let preview = QueuedMessagePreview {
        text: raw.clone(),
        input_source: Some(InputSource::Extension),
        custom_type: None,
    };
    assert!(is_coordination_preview(&preview));
    let message = js_json::from_value(
        &pi_js_compat::js_json!({"role":"user","content":raw,"inputSource":"extension","timestamp":0}),
    )
    .unwrap();
    assert!(is_coordination_message(&message));
}
