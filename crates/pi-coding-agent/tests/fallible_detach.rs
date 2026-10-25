#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::tool_detach_registry::{ToolDetachHandle, ToolDetachRegistry};
use pi_js_compat::{
    abort::AbortReason,
    callback_error::CallbackError,
    js_json as json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::sync::{Arc, Mutex};
struct ThrowingHandle {
    reason: CallbackError,
    calls: Arc<Mutex<Vec<String>>>,
}
impl ToolDetachHandle for ThrowingHandle {
    fn detach(&self) -> Result<bool, CallbackError> {
        self.calls.lock().unwrap().push("throwing".into());
        Err(self.reason.clone())
    }
}
fn snapshot(error: CallbackError) -> JsValue {
    let reason = error.thrown_value();
    let kind = if matches!(reason, AbortReason::Error(_) | AbortReason::DomException(_)) {
        "error"
    } else {
        "value"
    };
    json!({"kind":kind,"value":reason.value()})
}
#[test]
fn registry_synchronous_methods_preserve_original_callback_rejections() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let inputs = json!([{"kind":"error","value":raw},{"kind":"value","value":raw},{"kind":"value","value":null},{"kind":"value","value":{"message":raw}}]);
    let mut oracle = oracle::TsOracle::spawn(
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
    );
    for input in inputs.as_array().unwrap() {
        let expected=oracle.request(&json!({"id":1,"cases":[{"handles":[{"name":"throwing","result":false,"throw":input}],"steps":[{"op":"register","name":"throwing"},{"op":"running"},{"op":"all"}]}]}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let reason = if input["kind"] == json!("error") {
            AbortReason::Error(
                NativeNodeError::from_constructor(
                    NativeErrorClass::TypeError,
                    input["value"].as_string().unwrap().clone(),
                )
                .with_property("code", json!("E_DETACH")),
            )
        } else {
            AbortReason::Value(input["value"].clone())
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        let registry = Arc::new(ToolDetachRegistry::new());
        let _subscription = registry.register(Arc::new(ThrowingHandle {
            reason: CallbackError::from_rejection(reason),
            calls: calls.clone(),
        }));
        let running = snapshot(registry.detach_running().unwrap_err());
        assert_eq!(running, expected["output"]["histories"][0][1]["error"]);
        let all = snapshot(registry.detach_all().unwrap_err());
        assert_eq!(all, expected["output"]["histories"][0][2]["error"]);
        assert_eq!(
            json!(calls.lock().unwrap().clone()),
            expected["output"]["histories"][0][2]["calls"]
        );
        assert!(registry.has_running());
    }
}
