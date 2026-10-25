use crate::shared_oracle as oracle;

use pi_coding_agent::core::event_bus::{EventBus, create_event_bus};
use pi_js_compat::{
    abort::AbortReason,
    callback_error::CallbackError,
    js_json as json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};

#[test]
fn event_bus_catches_preserve_native_error_brand_properties_and_arbitrary_values() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let channel = JsString::from("event-") + &raw;
    let cases = json!([{ "kind":"error","value":raw },{"kind":"value","value":raw},{"kind":"value","value":null},{"kind":"value","value":{"message":raw,"code":"not-an-Error"}}]);
    let mut oracle = oracle::TsOracle::spawn(
        "event-bus-errors-oracle.ts",
        "packages/coding-agent/src/core/event-bus.ts",
    );
    let expected = oracle.request(&json!({"id":1,"channel":channel,"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let actual:Vec<_>=cases.as_array().unwrap().iter().map(|input| {
        let reason=if input["kind"]==json!("error") {
            AbortReason::Error(NativeNodeError::from_constructor(NativeErrorClass::TypeError,input["value"].as_string().unwrap().clone()).with_property("code",json!("E_FIXTURE")))
        } else { AbortReason::Value(input["value"].clone()) };
        let bus=create_event_bus();
        let _subscription=bus.on_handler(&channel,move |_| Err(CallbackError::from_rejection(reason.clone())));
        bus.emit(&channel,JsValue::Null).into_iter().map(|failure| {
            let reason=failure.error.thrown_value();
            let kind=if matches!(reason,AbortReason::Error(_)|AbortReason::DomException(_)) { "error" } else { "value" };
            json!({"prefix":JsString::from("Event handler error (")+&failure.channel+"):","reason":{"kind":kind,"value":reason.value()}})
        }).collect::<Vec<_>>()
    }).collect();
    assert_eq!(json!(actual), expected["output"]);
}
