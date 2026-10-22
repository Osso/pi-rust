use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

use pi_coding_agent::core::tools::edit_primitives::FuzzyMatchResult;

const POSITIVE_ZERO_BITS: u64 = 0x0000_0000_0000_0000;
const NEGATIVE_ZERO_BITS: u64 = 0x8000_0000_0000_0000;
const SMALLEST_POSITIVE_SUBNORMAL_BITS: u64 = 0x0000_0000_0000_0001;
const LARGEST_POSITIVE_FINITE_BITS: u64 = 0x7fef_ffff_ffff_ffff;
const LARGEST_NEGATIVE_FINITE_BITS: u64 = 0xffef_ffff_ffff_ffff;
const POSITIVE_INFINITY_BITS: u64 = 0x7ff0_0000_0000_0000;
const NEGATIVE_INFINITY_BITS: u64 = 0xfff0_0000_0000_0000;
const POSITIVE_NAN_PAYLOAD_ONE_BITS: u64 = 0x7ff8_0000_0000_0001;
const POSITIVE_NAN_PAYLOAD_FORTY_TWO_BITS: u64 = 0x7ff8_0000_0000_0042;
const NEGATIVE_NAN_PAYLOAD_FORTY_TWO_BITS: u64 = 0xfff8_0000_0000_0042;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn preserves_source_shaped_fields_and_debug() {
    let result = FuzzyMatchResult {
        found: true,
        index: 2.0,
        match_length: 5.0,
        used_fuzzy_match: false,
        content_for_replacement: JsString::from("before"),
    };

    assert!(result.found);
    assert_eq!(result.index, 2.0);
    assert_eq!(result.match_length, 5.0);
    assert!(!result.used_fuzzy_match);
    assert_eq!(result.content_for_replacement, "before");
    assert_debug(&result);
}

#[test]
fn preserves_empty_unicode_and_control_content() {
    let empty = FuzzyMatchResult {
        found: false,
        index: 0.0,
        match_length: 0.0,
        used_fuzzy_match: false,
        content_for_replacement: JsString::default(),
    };
    let content = FuzzyMatchResult {
        found: true,
        index: 1.0,
        match_length: 4.0,
        used_fuzzy_match: true,
        content_for_replacement: JsString::from("代理🙂\0\r\n"),
    };

    assert_eq!(empty.content_for_replacement, "");
    assert_eq!(content.content_for_replacement, "代理🙂\0\r\n");
}

#[test]
fn fields_are_independent_and_publicly_mutable() {
    let mut result = FuzzyMatchResult {
        found: false,
        index: 1.0,
        match_length: 2.0,
        used_fuzzy_match: true,
        content_for_replacement: JsString::from("before"),
    };

    result.found = true;
    result.index = -3.5;
    result.match_length = 8.25;
    result.used_fuzzy_match = false;
    result.content_for_replacement = JsString::from("after");

    assert!(result.found);
    assert_eq!(result.index, -3.5);
    assert_eq!(result.match_length, 8.25);
    assert!(!result.used_fuzzy_match);
    assert_eq!(result.content_for_replacement, "after");
}

#[test]
fn preserves_unrestricted_binary64_bits_in_numeric_fields() {
    const BIT_PATTERNS: [u64; 10] = [
        POSITIVE_ZERO_BITS,
        NEGATIVE_ZERO_BITS,
        SMALLEST_POSITIVE_SUBNORMAL_BITS,
        LARGEST_POSITIVE_FINITE_BITS,
        LARGEST_NEGATIVE_FINITE_BITS,
        POSITIVE_INFINITY_BITS,
        NEGATIVE_INFINITY_BITS,
        POSITIVE_NAN_PAYLOAD_ONE_BITS,
        POSITIVE_NAN_PAYLOAD_FORTY_TWO_BITS,
        NEGATIVE_NAN_PAYLOAD_FORTY_TWO_BITS,
    ];

    for bits in BIT_PATTERNS {
        let result = FuzzyMatchResult {
            found: true,
            index: f64::from_bits(bits),
            match_length: f64::from_bits(bits),
            used_fuzzy_match: false,
            content_for_replacement: JsString::from("content"),
        };

        assert_eq!(result.index.to_bits(), bits);
        assert_eq!(result.match_length.to_bits(), bits);
    }
}
