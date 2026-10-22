use pi_coding_agent::core::agent_session::{BashExecutionUpdateEvent, ExecuteBashOptions};
use pi_js_compat::js_json::{JsString, JsValue};
#[test]
fn bash_update_echoes_raw_rpc_request_id() {
    let id = JsString::from_utf16(vec![0xd800]);
    let options = ExecuteBashOptions {
        id: Some(id.clone()),
        ..Default::default()
    };
    let event = BashExecutionUpdateEvent {
        event_type: "bash_execution_update",
        id: options.id.clone(),
        delta: "chunk".into(),
    };
    let wire = pi_js_compat::js_json::to_value(&event).unwrap();
    assert_eq!(wire["id"], JsValue::String(id));
}
