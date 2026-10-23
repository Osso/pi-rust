//! Bash command IDs survive on their frozen RPC response, not an old-only chunk event.
use pi_coding_agent::modes::rpc::{rpc_types::RpcResponse, serialize_json_line};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

#[test]
fn bash_response_echoes_raw_rpc_request_id() {
    let id = JsString::from_utf16(vec![0xd800]);
    let response = RpcResponse::success_with_data(
        Some(id.clone()),
        "bash",
        pi_js_compat::js_json!({"output":"chunk","exitCode":0,"cancelled":false,"truncated":false}),
    );
    let wire = serialize_json_line(&response).unwrap();
    let decoded = parse_lossless(&wire).unwrap();
    assert_eq!(decoded["id"], JsValue::String(id));
    assert_eq!(decoded["type"].as_string().unwrap(), "response");
    assert_eq!(decoded["command"].as_string().unwrap(), "bash");
}
