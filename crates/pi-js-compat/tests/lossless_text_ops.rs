use pi_js_compat::js_json::JsString;

#[test]
fn search_slicing_and_replacement_operate_on_utf16_units() {
    let value = JsString::from("😀x😀");
    let low = JsString::from_utf16(vec![0xde00]);
    assert_eq!(value.find(&low), Some(1));
    assert_eq!(value.find_from(&low, 2), Some(4));
    assert_eq!(value.slice(1, 2), low);
    assert_eq!(value.replace(&low, "z").units(), &[0xd83d, 0x7a, 0x78, 0xd83d, 0x7a]);
    let mut edited = value.clone();
    edited.replace_range(1..2, "z");
    assert_eq!(edited.units(), &[0xd83d, 0x7a, 0x78, 0xd83d, 0xde00]);
    assert_eq!(JsString::from("abababac").find("ababac"), Some(2));
    assert_eq!(value.find(""), Some(0));
    assert_eq!(value.slice(99, 100), JsString::default());
}

#[test]
fn trim_suffix_and_inclusive_lines_never_replace_unmatched_units() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let value = JsString::from("\u{feff} ") + &raw + "\u{2029}";
    assert_eq!(value.trim_start(), raw.clone() + "\u{2029}");
    assert_eq!(value.trim_end(), JsString::from("\u{feff} ") + &raw);
    assert_eq!(value.strip_suffix("\u{2029}"), Some(JsString::from("\u{feff} ") + &raw));
    let lines = (raw.clone() + "\nlast").split_inclusive('\n').collect::<Vec<_>>();
    assert_eq!(lines, vec![raw + "\n", JsString::from("last")]);
}
