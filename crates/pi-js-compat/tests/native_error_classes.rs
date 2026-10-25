use crate::shared_oracle_0 as oracle;

use pi_js_compat::abort::AbortReason;
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

#[test]
fn native_error_causes_keep_branding_values_order_and_string_conversion() {
    let mut source = oracle::TsOracle::spawn("native-error-classes-oracle.ts", "packages/ai/src/types.ts");
    let nested = NativeNodeError::from_constructor(NativeErrorClass::TypeError, "nested")
        .with_cause(AbortReason::Value(pi_js_compat::js_json!({"message":"plain cause"})));
    let raw = JsString::from_utf16(vec![0xd800, 120, 0xdc00]);
    let cases = [
        (
            pi_js_compat::js_json!({"kind":"value","value":raw}),
            AbortReason::Value(JsValue::String(raw)),
        ),
        (
            pi_js_compat::js_json!({"kind":"value","value":null}),
            AbortReason::Value(JsValue::Null),
        ),
        (
            pi_js_compat::js_json!({"kind":"value","value":{"name":"TypeError","message":"lookalike"}}),
            AbortReason::Value(pi_js_compat::js_json!({"name":"TypeError","message":"lookalike"})),
        ),
        (
            pi_js_compat::js_json!({"kind":"error","constructor":"TypeError","message":"nested","cause":{"kind":"value","value":{"message":"plain cause"}}}),
            AbortReason::Error(nested),
        ),
        (
            pi_js_compat::js_json!({"kind":"dom","name":"AbortError","message":"cancel"}),
            AbortReason::DomException(
                NativeNodeError::new("AbortError", "cancel").with_property("code", JsValue::Number(20.0)),
            ),
        ),
    ];
    for (spec, cause) in cases {
        let error = NativeNodeError::new("Error", "outer")
            .with_property("before", JsValue::Number(1.0))
            .with_cause(cause.clone())
            .with_property("after", JsValue::Number(2.0));
        assert_eq!(error.cause(), Some(&cause));
        assert_eq!(error.clone(), error);
        let cause_string = match &cause {
            AbortReason::Value(value) => pi_js_compat::js_json::to_js_string(value).unwrap(),
            AbortReason::Error(error) | AbortReason::DomException(error) => error.string_value(),
        };
        let expected = source.request(&pi_js_compat::js_json!({"operation":"cause","cause":spec}));
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let actual = pi_js_compat::js_json!({
            "value":error.value(), "causeError":cause.as_error().is_some(),
            "causeDom":matches!(cause, AbortReason::DomException(_)), "causeString":cause_string,
        });
        assert_eq!(actual, expected["output"], "spec={spec:?}");
        assert_eq!(
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected["output"])
        );
        let replaced = error.with_property("cause", JsValue::Null);
        assert_eq!(replaced.cause(), Some(&AbortReason::Value(JsValue::Null)));
    }
    let cause = NativeNodeError::from_constructor(NativeErrorClass::TypeError, "nested");
    let branded = NativeNodeError::new("Error", "outer").with_cause(cause.clone().into());
    let plain = NativeNodeError::new("Error", "outer").with_property("cause", cause.value());
    assert_eq!(branded.value(), plain.value());
    assert_ne!(branded, plain);
}
