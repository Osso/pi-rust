//! Differential proof for `Number`, `parseInt`, `parseFloat`, and `Number::toString`.

use crate::shared_oracle_2 as oracle;

use pi_js_compat::number::{js_number_to_string, js_number_value, parse_float, parse_int, parse_number, to_fixed};
use serde_json::json;

const NUMBER_VALUES: &[&str] = &[
    "",
    "  ",
    "\u{a0}\u{feff}12\u{2028}",
    "0",
    "-0",
    "1.5",
    " 42 ",
    ".5",
    "5.",
    "1e3",
    "1E-2",
    "+7",
    "-7",
    "0x1F",
    "0X1f",
    "0b101",
    "0o17",
    "Infinity",
    "-Infinity",
    "+Infinity",
    "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
    "\u{85}",
    "-0x10",
    "0x",
    "inf",
    "nan",
    "NaN",
    "1_000",
    "1 2",
    "12px",
    "1e",
    "+-1",
    "1e999",
    "-1e999",
    "0.000001",
    "1e-7",
];

const FLOAT_VALUES: &[&str] = &[
    "  12px",
    "1.5e3ms",
    "+7",
    "-12.5",
    "0x10",
    ".5",
    "5.",
    "1e",
    "1e+",
    "1,5",
    "0b101",
    "12 34",
    "\u{a0}3.25",
    "\u{feff}2",
    "Infinityx",
    "-Infinity",
    "+Infinity",
    "1e999",
    "-1e999",
    "",
    "   ",
    "abc",
    "infinity",
    "NaN",
    ".",
    "-",
    "\u{85}3",
    "e3",
];

const INT_VALUES: &[(&str, u32)] = &[
    ("", 10),
    ("   ", 10),
    (" 42 ", 10),
    ("+7", 10),
    ("-7", 10),
    ("-0", 10),
    ("1abc", 10),
    ("1.9", 10),
    ("0x10", 10),
    ("0x10", 16),
    ("0Xf", 16),
    ("0x10", 0),
    ("010", 0),
    ("z", 36),
    ("Z", 36),
    ("2", 2),
    ("11", 2),
    ("7", 1),
    ("7", 37),
    ("+ 1", 10),
    ("\u{a0}\u{feff}12\u{2028}", 10),
];

const NUMBER_STRINGS: &[&str] = &[
    "0",
    "-0",
    "1",
    "-1",
    "1.5",
    "0.1",
    "0.30000000000000004",
    "1e20",
    "1e21",
    "1.234e21",
    "1e-6",
    "1e-7",
    "5e-324",
    "1.7976931348623157e308",
    "123456789012345680000",
    "100",
    "1e300",
    "1e-300",
    "9007199254740993",
    "1.2345678901234568e21",
    "0.000001",
    "0.0000001",
    "-1e-7",
    "1e999",
    "-1e999",
    "NaN",
];

fn normalize_parsed(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value == 0.0 && value.is_sign_negative() {
        return "-0".to_owned();
    }
    js_number_to_string(value)
}

#[test]
fn matches_node_number_conversions() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let int_values: Vec<_> = INT_VALUES.iter().map(|(value, radix)| json!([value, radix])).collect();
    let response = oracle.request(&json!({
        "id": 10,
        "operation": "number",
        "number_values": NUMBER_VALUES,
        "float_values": FLOAT_VALUES,
        "int_values": int_values,
        "number_strings": NUMBER_STRINGS,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let expected_number: Vec<String> = response["output"]["number"]
        .as_array()
        .expect("number array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    let actual_number: Vec<String> = NUMBER_VALUES
        .iter()
        .map(|value| normalize_parsed(parse_number(value)))
        .collect();
    assert_eq!(actual_number, expected_number, "Number(string)");

    let expected_float: Vec<String> = response["output"]["float"]
        .as_array()
        .expect("float array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    let actual_float: Vec<String> = FLOAT_VALUES
        .iter()
        .map(|value| normalize_parsed(parse_float(value)))
        .collect();
    assert_eq!(actual_float, expected_float, "parseFloat(string)");

    let expected_int: Vec<String> = response["output"]["int"]
        .as_array()
        .expect("int array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    let actual_int: Vec<String> = INT_VALUES
        .iter()
        .map(|(value, radix)| normalize_parsed(parse_int(&value.encode_utf16().collect::<Vec<_>>(), *radix)))
        .collect();
    assert_eq!(actual_int, expected_int, "parseInt(string, radix)");

    let expected_strings: Vec<String> = response["output"]["strings"]
        .as_array()
        .expect("strings array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    let actual_strings: Vec<String> = NUMBER_STRINGS
        .iter()
        .map(|value| js_number_to_string(parse_number(value)))
        .collect();
    assert_eq!(actual_strings, expected_strings, "String(Number(string))");
}

#[test]
fn parse_int_matches_node_for_raw_utf16_prefixes() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let mut cases = vec![
        (vec![0xd800, 0x31], 10),
        (vec![0xdc00, 0x31], 10),
        (vec![0xfeff, 0x2d, 0x30, 0xd800], 10),
        (vec![0x85, 0x31], 10),
        (vec![0xfeff, 0x30, 0x78, 0x66, 0xdc00], 0),
        (vec![0x31], 1),
        (vec![0x31], 37),
    ];
    for prefix in ["12", "0xF", "-0", "+12", "0Xff"] {
        for suffix in [vec![0xd800], vec![0xdc00], vec![0xd83d, 0xde00]] {
            let units = prefix.encode_utf16().chain(suffix).collect::<Vec<_>>();
            for radix in [0, 10, 16] {
                cases.push((units.clone(), radix));
            }
        }
    }
    let response=oracle.request(&json!({"id":"raw-parse-int","operation":"number","number_values":[],"float_values":[],"int_values":[],"number_strings":[],"int_utf16_values":cases}));
    assert_eq!(response["ok"], true, "{response}");
    let expected = response["output"]["int_utf16"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let actual = cases
        .iter()
        .map(|(units, radix)| normalize_parsed(parse_int(units, *radix)))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

const FIXED_CASES: &[(f64, u32)] = &[
    (0.0, 1),
    (-0.0, 1),
    (1.0, 1),
    (1.25, 1),
    (1.15, 1),
    (1.005, 2),
    (2.5, 0),
    (-2.5, 0),
    (1.234, 1),
    (4.567, 1),
    (31.234, 1),
    (286.705, 1),
    (26.667, 1),
    (999.0, 1),
    (1234.0, 0),
    (0.05, 1),
    (0.15, 1),
    (123456789.9876, 3),
    (3.5, 0),
    (0.5, 0),
    (0.25, 1),
    (99.95, 1),
    (1e-7, 1),
    (-0.0001, 2),
    (123456789012345.0, 1),
    (999.99, 0),
    (0.000001, 3),
];

const JSON_STRINGIFY_VALUES: &[&str] = &[
    "0",
    "-0",
    "1",
    "-1",
    "42.5",
    "3000",
    "0.1",
    "0.30000000000000004",
    "9007199254740991",
    "9007199254740992",
    "9007199254740994",
    "-9007199254740994",
    "18446744073709551615",
    "18446744073709551616",
    "100000000000000000000",
    "-100000000000000000000",
    "1e21",
    "1.234e21",
    "-1e21",
    "1e20",
    "1e-6",
    "1e-7",
    "5e-324",
    "1.7976931348623157e308",
    "1e999",
    "-1e999",
    "NaN",
];

#[test]
fn matches_node_json_stringify_numbers() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let response = oracle.request(&json!({
        "id": 12,
        "operation": "json_stringify",
        "values": JSON_STRINGIFY_VALUES,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected: Vec<String> = response["output"]
        .as_array()
        .expect("stringify array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    // Compare JSON values: `serde_json` spells exponents without the JavaScript
    // `+` (`1e21` vs `1e+21`), which is a workspace-wide serialization spelling,
    // not a numeric difference.
    let actual: Vec<serde_json::Value> = JSON_STRINGIFY_VALUES
        .iter()
        .map(|value| js_number_value(parse_number(value)))
        .collect();
    let expected: Vec<serde_json::Value> = expected
        .iter()
        .map(|value| serde_json::from_str(value).expect("JSON.stringify output is JSON"))
        .collect();
    assert_eq!(actual, expected, "JSON.stringify(number)");
}

#[test]
fn matches_node_to_fixed() {
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let cases: Vec<_> = FIXED_CASES
        .iter()
        .map(|(value, digits)| json!([value, digits]))
        .collect();
    let response = oracle.request(&json!({
        "id": 11,
        "operation": "to_fixed",
        "cases": cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected: Vec<String> = response["output"]
        .as_array()
        .expect("fixed array")
        .iter()
        .map(|value| value.as_str().expect("string").to_owned())
        .collect();
    let actual: Vec<String> = FIXED_CASES
        .iter()
        .map(|(value, digits)| to_fixed(*value, *digits))
        .collect();
    assert_eq!(actual, expected, "Number.prototype.toFixed");
}
