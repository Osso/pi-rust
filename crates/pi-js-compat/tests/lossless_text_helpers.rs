use pi_js_compat::js_json::{JsString, JsValue};

#[test]
fn lowercase_retains_surrogates_and_respects_unicode_context() {
    let value = JsString::from_utf16(vec![0x039f, 0x03a3, 0xd800, 0x0041, 0xdfff]);
    assert_eq!(value.to_lowercase().units(), &[0x03bf, 0x03c2, 0xd800, 0x0061, 0xdfff]);
    assert_eq!(value.units(), &[0x039f, 0x03a3, 0xd800, 0x0041, 0xdfff]);
}

#[test]
fn unsigned_number_projection_rejects_fractional_negative_and_out_of_range_values() {
    assert_eq!(JsValue::Number(42.0).as_u64(), Some(42));
    for value in [-1.0, 0.5, f64::INFINITY, 18446744073709551616.0, f64::NAN] {
        assert_eq!(JsValue::Number(value).as_u64(), None);
    }
}
