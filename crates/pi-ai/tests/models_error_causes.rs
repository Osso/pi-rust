use crate::shared_oracle as oracle;
use pi_ai::auth::{ModelsError, ModelsErrorCode};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};

#[test]
fn models_error_preserves_exact_message_raw_cause_and_native_source() {
    let mut source = oracle::TsOracle::spawn("models-error-oracle.ts", "packages/ai/src/auth/resolve.ts");
    let raw = JsValue::String(JsString::from_utf16(vec![100, 0xd800]));
    for (index, cause) in [JsValue::Null, JsValue::Bool(false), raw.clone(), json!({"nested":raw})]
        .into_iter()
        .enumerate()
    {
        let expected = source.request(&json!({"id":index,"cause":cause}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let error = ModelsError::with_cause(
            ModelsErrorCode::ModelSource,
            "refresh failed",
            Some(Box::new(AbortReason::Value(cause.clone()))),
        );
        assert_eq!(error.message, "refresh failed");
        assert_eq!(error.cause.as_deref().unwrap().value(), cause);
        let callback = error.into_callback();
        assert_eq!(callback.name, "ModelsError");
        assert_eq!(callback.thrown_value().value(), expected["output"]["value"]);
        assert!(
            callback
                .original
                .as_ref()
                .unwrap()
                .downcast_ref::<ModelsError>()
                .is_some()
        );
    }
    let native = NativeNodeError::from_constructor(NativeErrorClass::TypeError, "native detail")
        .with_cause(AbortReason::Value(raw.clone()))
        .with_property("code", JsValue::String("E_FIXTURE".into()));
    let expected = source.request(&json!({"id":5,"kind":"native","cause":raw}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let error = ModelsError::with_cause(
        ModelsErrorCode::ModelSource,
        "refresh failed",
        Some(Box::new(native.clone().into())),
    );
    assert_eq!(
        std::error::Error::source(&error)
            .unwrap()
            .downcast_ref::<NativeNodeError>(),
        Some(&native)
    );
    let callback = error.into_callback();
    assert_eq!(callback.thrown_value().value(), expected["output"]["value"]);
    let cause = callback
        .thrown_value()
        .as_error()
        .unwrap()
        .cause()
        .unwrap()
        .as_error()
        .unwrap();
    assert_eq!(cause.constructor_class(), NativeErrorClass::TypeError);
    assert_eq!(cause, native);
}
