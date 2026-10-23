use pi_js_compat::abort::AbortReason;
use pi_js_compat::js_json::JsValue;

pub fn describe(value: Option<AbortReason>) -> JsValue {
    match value {
        None => pi_js_compat::js_json!({"kind":"undefined"}),
        Some(AbortReason::Value(JsValue::Number(number))) => {
            let text = if number == 0.0 && number.is_sign_negative() {
                "-0".into()
            } else {
                pi_js_compat::number::js_number_to_string(number)
            };
            pi_js_compat::js_json!({"kind":"number","value":text})
        }
        Some(AbortReason::Value(value)) => pi_js_compat::js_json!({"kind":"value","value":value}),
        Some(AbortReason::Error(error)) => pi_js_compat::js_json!({"kind":"error","value":error.value()}),
        Some(AbortReason::DomException(error)) => pi_js_compat::js_json!({"kind":"dom","value":error.value()}),
    }
}
