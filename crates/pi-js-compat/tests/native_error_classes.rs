#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};

#[test]
fn constructor_identity_matches_node_independently_of_mutable_name() {
    use NativeErrorClass::*;
    let classes = [
        Error,
        EvalError,
        RangeError,
        ReferenceError,
        SyntaxError,
        TypeError,
        URIError,
        AggregateError,
    ];
    let mut oracle = oracle::TsOracle::spawn("native-error-classes-oracle.ts", "packages/ai/src/types.ts");
    for constructor in classes {
        for name in [None, Some("TypeError"), Some("renamed")] {
            let message = JsString::from_utf16(vec![b'm' as u16, 0xdc00]);
            let mut request = pi_js_compat::js_json!({"id":"class","constructor":constructor.name(),"message":message});
            let mut native = NativeNodeError::from_constructor(constructor, message);
            if let Some(name) = name {
                let name = if name.starts_with("renamed") {
                    JsString::from_utf16("renamed".encode_utf16().chain([0xd800]).collect())
                } else {
                    name.into()
                };
                request
                    .as_object_mut()
                    .unwrap()
                    .insert("name".into(), JsValue::String(name.clone()));
                native = native.with_property("name", JsValue::String(name));
            }
            let expected = oracle.request(&request);
            assert_eq!(expected["ok"], JsValue::Bool(true));
            let instances: JsObject = classes
                .into_iter()
                .map(|class| (class.name().into(), JsValue::Bool(native.is_instance_of(class))))
                .collect();
            assert_eq!(
                pi_js_compat::js_json!({"value":native.value(),"instances":JsValue::Object(instances)}),
                expected["output"]
            );
            assert_eq!(native.constructor_class(), constructor);
        }
    }
    let plain = NativeNodeError::new("TypeError", "renamed plain Error");
    assert_eq!(plain.constructor_class(), NativeErrorClass::Error);
    assert!(!plain.is_instance_of(NativeErrorClass::TypeError));
    assert!(plain.is_instance_of(NativeErrorClass::Error));
}
