use pi_js_compat::string::utf16_slice_to_utf8;

#[test]
fn slicing_preserves_utf16_boundaries_before_sqlite_utf8_conversion() {
    for (value, start, end, expected) in [
        ("abc", 0, 2, "ab"),
        ("界🙂z", 1, 3, "🙂"),
        ("界🙂z", 0, 2, "界�"),
        ("🙂", 1, 2, "�"),
        ("x", 2, 5, ""),
        ("abc", 2, 1, ""),
    ] {
        assert_eq!(utf16_slice_to_utf8(value, start, end), expected);
    }
}
