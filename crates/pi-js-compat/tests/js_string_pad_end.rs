use pi_js_compat::js_json::JsString;

#[test]
fn pad_end_counts_utf16_units_and_never_truncates() {
    let lone = JsString::from_utf16(vec![u16::from(b'a'), 0xD800]);
    let mut expected = lone.clone();
    expected.push_str("   ");
    assert_eq!(lone.pad_end(5), expected);
    assert_eq!(JsString::from("\u{1F600}").pad_end(3), JsString::from("\u{1F600} "));
    assert_eq!(JsString::from("abcdef").pad_end(3), JsString::from("abcdef"));
}
