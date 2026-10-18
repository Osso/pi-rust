//! `parse_number` against values printed by Node 26 `Number(string)`.

use pi_ai::js_number::{date_parse, parse_float, parse_number};

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

/// Values printed by Node 26 `Date.parse(string)` with `TZ=UTC`.
#[test]
fn date_parse_matches_javascript_for_http_and_iso_dates() {
    let cases: &[(&str, f64)] = &[
        ("Sun, 06 Nov 1994 08:49:37 GMT", 784_111_777_000.0),
        ("Sunday, 06-Nov-94 08:49:37 GMT", 784_111_777_000.0),
        ("Sun Nov  6 08:49:37 1994", 784_111_777_000.0),
        ("2020-01-01T00:00:00Z", 1_577_836_800_000.0),
        ("2020-01-01T00:00:00.123Z", 1_577_836_800_123.0),
        ("2020-01-01T00:00:00.123456789Z", 1_577_836_800_123.0),
        ("2020-01-01T00:00:00+02:00", 1_577_829_600_000.0),
        ("2020-01-01T00:00:00+0200", 1_577_829_600_000.0),
        ("2020-01-01T00:00:00-02:30", 1_577_845_800_000.0),
        ("2020-01-01T00:00Z", 1_577_836_800_000.0),
        ("2020-01-01 00:00:00Z", 1_577_836_800_000.0),
        ("2020-01-01t00:00:00z", 1_577_836_800_000.0),
        ("2020-01-01", 1_577_836_800_000.0),
        ("2020-01", 1_577_836_800_000.0),
        ("2020", 1_577_836_800_000.0),
        ("2020-1-1", 1_577_836_800_000.0),
        ("2020-01-1", 1_577_836_800_000.0),
        ("2020-02-30T00:00:00Z", 1_583_020_800_000.0),
        ("2020-04-31T00:00:00Z", 1_588_291_200_000.0),
        ("2020-01-01T24:00:00Z", 1_577_923_200_000.0),
        ("2020-01-01T24:00Z", 1_577_923_200_000.0),
        ("0000-01-01T00:00:00Z", -62_167_219_200_000.0),
        ("+002020-01-01T00:00:00Z", 1_577_836_800_000.0),
        ("-000001-01-01T00:00:00Z", -62_198_755_200_000.0),
        ("+275760-09-13T00:00:00Z", 8_640_000_000_000_000.0),
        ("-271821-04-20T00:00:00Z", -8_640_000_000_000_000.0),
    ];
    for (input, expected) in cases {
        let parsed = date_parse(input);
        assert_eq!(parsed.to_bits(), expected.to_bits(), "Date.parse({input:?})");
    }
}

#[test]
fn date_parse_rejects_inputs_javascript_rejects() {
    for input in [
        "",
        "not a date",
        "2020-13-01T00:00:00Z",
        "2020-00-01T00:00:00Z",
        "2020-01-00T00:00:00Z",
        "2020-01-32T00:00:00Z",
        "2020-01-01T00:60:00Z",
        "2020-01-01T23:59:60Z",
        "2020-01-01T24:00:01Z",
        "2020-01-01T24:00:00.5Z",
        "2020-01-01T1:00Z",
        "2020-01-01T01:0Z",
        "2020-01-01T00:00:00+24:00",
        "2020-01-01T00:00:00+00:60",
        "2020-01-01T00:00:00+02",
        "2020-01-01T00:00:00.Z",
        " 2020-01-01T00:00:00Z ",
        "+999999-01-01T00:00:00Z",
        "-999999-01-01T00:00:00Z",
        "+275760-09-13T00:00:00.001Z",
        "-271821-04-19T23:59:59.999Z",
    ] {
        assert!(date_parse(input).is_nan(), "Date.parse({input:?}) should be NaN");
    }
}
