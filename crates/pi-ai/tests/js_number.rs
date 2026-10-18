//! `parse_number` against values printed by Node 26 `Number(string)`.

use pi_ai::js_number::parse_number;

#[test]
fn parses_strings_like_javascript_number() {
    let cases: &[(&str, f64)] = &[
        ("", 0.0),
        ("  ", 0.0),
        ("\u{a0}\u{feff}12\u{2028}", 12.0),
        ("0", 0.0),
        ("-0", -0.0),
        ("1.5", 1.5),
        (" 42 ", 42.0),
        (".5", 0.5),
        ("5.", 5.0),
        ("1e3", 1000.0),
        ("1E-2", 0.01),
        ("+7", 7.0),
        ("-7", -7.0),
        ("0x1F", 31.0),
        ("0X1f", 31.0),
        ("0b101", 5.0),
        ("0o17", 15.0),
        ("Infinity", f64::INFINITY),
        ("-Infinity", f64::NEG_INFINITY),
        ("+Infinity", f64::INFINITY),
        ("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", 8.711228593176025e40),
    ];
    for (input, expected) in cases {
        let parsed = parse_number(input);
        assert_eq!(parsed.to_bits(), expected.to_bits(), "Number({input:?})");
    }
}

#[test]
fn rejects_strings_javascript_number_rejects() {
    for input in [
        "\u{85}", "-0x10", "0x", "inf", "nan", "NaN", "1_000", "1 2", "12px", "1e", "+-1",
    ] {
        assert!(parse_number(input).is_nan(), "Number({input:?}) should be NaN");
    }
}
