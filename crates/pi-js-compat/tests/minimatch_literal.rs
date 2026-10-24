use pi_js_compat::{
    js_json::JsString,
    minimatch::{Minimatch, MinimatchOptions},
};

#[test]
fn literal_segments_retain_native_units_and_unescape_without_accepting_patterns() {
    for (pattern, expected) in [
        ("file.ts", "file.ts"),
        (r"literal\*.ts", "literal*.ts"),
        (r"quoted\ name", "quoted name"),
    ] {
        let matcher = Minimatch::new(pattern, MinimatchOptions::default());
        let literal = matcher.literal_segment().expect("literal segment");
        assert_eq!(literal, expected);
        assert!(matcher.matches(&literal, false));
    }
    let raw = JsString::from_utf16(vec![78, 0xd800, 0xdc00, 0xdc00]);
    let matcher = Minimatch::new(&raw, MinimatchOptions::default());
    assert_eq!(matcher.literal_segment(), Some(raw.clone()));
    assert!(matcher.matches(raw, false));
    for pattern in ["*.ts", "**", "a/b", "[ab]", "!file", "#comment", ""] {
        assert_eq!(
            Minimatch::new(pattern, MinimatchOptions::default()).literal_segment(),
            None,
            "{pattern}"
        );
    }
}
