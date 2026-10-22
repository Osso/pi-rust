//! Lossless persisted Pyrun payloads; real bridge/runner types, no DB harness.
use pi_coding_agent::bundled_extensions::pyrun::detached_bridge::DetachedPyrunBridgeRequest;
use pi_js_compat::js_json::{self, JsString};

#[test]
fn persisted_bridge_request_retains_raw_method_ids_and_params() {
    let wire = r#"{"command":"request","identity":{"jobId":"job-\ud800","owner":{"sessionId":"session-\udfff","agentId":null},"processIdentity":{"pid":1,"startTimeTicks":2},"outputLabel":"output-\ud801"},"method":"unknown-\ud802","params":{"arg-\ud803":"value-\ud804"},"protocol":"pyrun-bridge-v1","replyTo":{"sessionId":"session-\udfff","agentId":"runner-\ud805"},"requestId":"request-\ud806","toolCallId":"tool-\ud807"}"#;
    let value = js_json::parse_lossless(wire).unwrap();
    let request: DetachedPyrunBridgeRequest = js_json::from_value(&value).unwrap();
    assert_eq!(js_json::to_value(&request).unwrap(), value);
    assert_eq!(
        request.method,
        JsString::from("unknown-") + JsString::from_utf16(vec![0xd802])
    );
}
