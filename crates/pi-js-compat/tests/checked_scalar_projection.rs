use pi_js_compat::{
    js_json as json,
    js_json::{JsObject, JsValue},
};

#[test]
fn checked_scalar_projection_rejects_nonfinite_numbers_at_any_depth() {
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let value = JsValue::Number(number);
        for payload in [
            value.clone(),
            JsValue::Array(vec![value.clone()]),
            JsValue::Object(JsObject::from([(
                "nested".into(),
                JsValue::Array(vec![JsValue::Object(JsObject::from([("number".into(), value)]))]),
            )])),
        ] {
            assert!(
                payload.to_serde().is_err(),
                "checked projection must not silently return null"
            );
        }
    }
    assert_eq!(
        json!({"number":1.5,"text":"scalar"}).to_serde().unwrap(),
        serde_json::json!({"number":1.5,"text":"scalar"})
    );
}
