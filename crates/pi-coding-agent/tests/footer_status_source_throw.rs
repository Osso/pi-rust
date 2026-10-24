#[path = "support/footer_callable_source.rs"]
mod source;
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

#[test]
fn actual_frozen_tui_scheduled_footer_callback_throw_is_uncaught() {
    let directory = tempfile::tempdir().unwrap();
    let output = source::run(
        "uncaught",
        directory.path(),
        pi_js_compat::js_json!({"id":"faux-1","provider":"faux","reasoning":false,"contextWindow":100000}),
    );
    assert_eq!(
        output.status.code(),
        Some(17),
        "source stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = parse_lossless(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    assert_eq!(value["uncaught"], JsValue::Bool(true));
    assert_eq!(value["sameError"], JsValue::Bool(true));
    assert_eq!(value["trace"], pi_js_compat::js_json!([null]));
    assert_eq!(
        value["error"],
        pi_js_compat::js_json!({"name":"TypeError","message":JsString::from("status-")+JsString::from_utf16(vec![0xd800]),"code":"ESTATUS"})
    );
}
