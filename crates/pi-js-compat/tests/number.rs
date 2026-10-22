//! `Number`, `parseInt`, `parseFloat`, and `Number::toString` parity.

use pi_js_compat::number::{js_number_to_string, parse_float, parse_int, parse_number};

#[test]
fn parses_raw_utf16_prefix_without_replacing_surrogates() {
    assert_eq!(parse_int(&[0xfeff, 0x31, 0x32, 0xd800], 10), 12.0);
    assert_eq!(parse_int(&[0x31, 0x32, 0xdc00], 10), 12.0);
    assert!(parse_int(&[0xd800, 0x31], 10).is_nan());
}

#[test]
fn parses_strings_like_javascript_parse_int() {
    let cases: &[(&str, u32, f64)] = &[
        ("", 10, f64::NAN),
        ("   ", 10, f64::NAN),
        (" 42 ", 10, 42.0),
        ("+7", 10, 7.0),
        ("-7", 10, -7.0),
        ("-0", 10, -0.0),
        ("1abc", 10, 1.0),
        ("1.9", 10, 1.0),
        ("0x10", 10, 0.0),
        ("0x10", 16, 16.0),
        ("0Xf", 16, 15.0),
        ("0x10", 0, 16.0),
        ("010", 0, 10.0),
        ("z", 36, 35.0),
        ("Z", 36, 35.0),
        ("2", 2, f64::NAN),
        ("11", 2, 3.0),
        ("7", 1, f64::NAN),
        ("7", 37, f64::NAN),
        ("+ 1", 10, f64::NAN),
        ("\u{a0}\u{feff}12\u{2028}", 10, 12.0),
    ];
    for (input, radix, expected) in cases {
        let parsed = parse_int(&input.encode_utf16().collect::<Vec<_>>(), *radix);
        assert_eq!(
            parsed.to_bits(),
            expected.to_bits(),
            "Number.parseInt({input:?}, {radix})"
        );
    }
}

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

/// Values printed by Node 26 `parseFloat(string)`.
#[test]
fn parse_float_matches_javascript_prefix_parsing() {
    let cases: &[(&str, f64)] = &[
        ("  12px", 12.0),
        ("1.5e3ms", 1500.0),
        ("+7", 7.0),
        ("-12.5", -12.5),
        ("0x10", 0.0),
        (".5", 0.5),
        ("5.", 5.0),
        ("1e", 1.0),
        ("1e+", 1.0),
        ("1,5", 1.0),
        ("0b101", 0.0),
        ("12 34", 12.0),
        ("\u{a0}3.25", 3.25),
        ("\u{feff}2", 2.0),
        ("Infinityx", f64::INFINITY),
        ("-Infinity", f64::NEG_INFINITY),
        ("+Infinity", f64::INFINITY),
        ("1e999", f64::INFINITY),
        ("-1e999", f64::NEG_INFINITY),
    ];
    for (input, expected) in cases {
        let parsed = parse_float(input);
        assert_eq!(parsed.to_bits(), expected.to_bits(), "parseFloat({input:?})");
    }
}

#[test]
fn parse_float_rejects_inputs_javascript_rejects() {
    for input in ["", "   ", "abc", "infinity", "NaN", ".", "-", "\u{85}3", "e3"] {
        assert!(parse_float(input).is_nan(), "parseFloat({input:?}) should be NaN");
    }
}

#[test]
fn formats_numbers_like_ecma_262_number_to_string() {
    let cases: &[(f64, &str)] = &[
        (0.0, "0"),
        (-0.0, "0"),
        (1.0, "1"),
        (-1.0, "-1"),
        (1.5, "1.5"),
        (100.0, "100"),
        (0.1, "0.1"),
        (0.30000000000000004, "0.30000000000000004"),
        (1e20, "100000000000000000000"),
        (1e21, "1e+21"),
        (1.234e21, "1.234e+21"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
        (5e-324, "5e-324"),
        (1.7976931348623157e308, "1.7976931348623157e+308"),
        (123456789012345680000.0, "123456789012345680000"),
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
    ];
    for (value, expected) in cases {
        assert_eq!(js_number_to_string(*value), *expected, "{value}");
    }
}
