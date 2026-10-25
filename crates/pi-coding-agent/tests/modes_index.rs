use std::sync::{Arc, Mutex};

use pi_coding_agent::modes::{ModelInfo, RpcEventListener};
use pi_js_compat::js_json::JsValue;
use serde_json::json;

#[test]
fn public_modes_forward_rpc_model_and_listener_contracts() {
    let wire = json!({"provider":"faux", "id":"native", "contextWindow":8192, "reasoning":true, "extra":"kept"});
    let model: ModelInfo = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(model.provider, "faux");
    assert_eq!(model.context_window, 8192.0);
    assert_eq!(serde_json::to_value(&model).unwrap(), wire);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::clone(&observed);
    let listener: RpcEventListener = Arc::new(move |event| {
        events.lock().unwrap().push(event.clone());
        Ok(())
    });
    let event = JsValue::from(json!({"type":"agent_end", "messages":[]}));
    listener(&event).unwrap();
    assert_eq!(*observed.lock().unwrap(), [event]);
}
