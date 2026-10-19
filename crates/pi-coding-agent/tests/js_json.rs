use pi_js_compat::js_json::{object_entries, parse, stringify, stringify_pretty};
use serde_json::json;

fn parse_error(text: &str) -> String {
    parse(text).expect_err("input must be rejected").to_string()
}

#[test]
fn parse_errors_use_v8_messages_with_positions() {
    let cases = [
        ("", "Unexpected end of JSON input"),
        ("[", "Unexpected end of JSON input"),
        (
            "{",
            "Expected property name or '}' in JSON at position 1 (line 1 column 2)",
        ),
        (
            "{\"a\"",
            "Expected ':' after property name in JSON at position 4 (line 1 column 5)",
        ),
        (
            "{\"a\":1",
            "Expected ',' or '}' after property value in JSON at position 6 (line 1 column 7)",
        ),
        (
            "{\"a\":1,}",
            "Expected double-quoted property name in JSON at position 7 (line 1 column 8)",
        ),
        (
            "[1 2]",
            "Expected ',' or ']' after array element in JSON at position 3 (line 1 column 4)",
        ),
        ("[1,]", "Unexpected token ']', \"[1,]\" is not valid JSON"),
        (
            "true x",
            "Unexpected non-whitespace character after JSON at position 5 (line 1 column 6)",
        ),
        (
            "-",
            "No number after minus sign in JSON at position 1 (line 1 column 2)",
        ),
        (
            "1.e5",
            "Unterminated fractional number in JSON at position 2 (line 1 column 3)",
        ),
        (
            "1e+",
            "Exponent part is missing a number in JSON at position 3 (line 1 column 4)",
        ),
        ("[-01]", "Unexpected number in JSON at position 3 (line 1 column 4)"),
        ("\"abc", "Unterminated string in JSON at position 4 (line 1 column 5)"),
        ("\"a\\", "Unexpected end of JSON input"),
        (
            "\"a\\x\"",
            "Bad escaped character in JSON at position 3 (line 1 column 4)",
        ),
        (
            "\"a\nb\"",
            "Bad control character in string literal in JSON at position 2 (line 1 column 3)",
        ),
        (
            "\"\\u12\"",
            "Bad Unicode escape in JSON at position 5 (line 1 column 6)",
        ),
        ("NaN", "\"NaN\" is not valid JSON"),
        ("[nulL]", "Unexpected token 'L', \"[nulL]\" is not valid JSON"),
        (
            "xxxxxxxxxxxxxxxxxxxxx",
            "Unexpected token 'x', \"xxxxxxxxxx\"... is not valid JSON",
        ),
        (
            "{\"key\": [1, 2, 3], \"other\": {\"nested\": tru}}",
            "Unexpected token '}', ...\"sted\": tru}}\" is not valid JSON",
        ),
        (
            "{\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\": x}",
            "Unexpected token 'x', ...\"aaaaaaa\": x}\" is not valid JSON",
        ),
        (
            "{\r\"a\":1\r\r,x}",
            "Expected double-quoted property name in JSON at position 10 (line 4 column 2)",
        ),
        (
            "[1,\r\n2 3]",
            "Expected ',' or ']' after array element in JSON at position 7 (line 2 column 3)",
        ),
        (
            "{\"😀\": [1 x]}",
            "Expected ',' or ']' after array element in JSON at position 10 (line 1 column 11)",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(parse_error(input), expected, "input {input:?}");
    }
}

#[test]
fn parse_keeps_last_duplicate_value_at_first_position() {
    let value = parse("{\"a\":1,\"b\":2,\"a\":3}").unwrap();
    assert_eq!(stringify(&value), "{\"a\":3,\"b\":2}");
}

#[test]
fn stringify_orders_array_index_keys_first_like_javascript_objects() {
    let value = parse("{\"b\":1,\"a\":2,\"1\":3,\"0\":4,\"01\":5,\"4294967295\":6,\"4294967294\":7,\"-1\":8}").unwrap();
    assert_eq!(
        stringify(&value),
        "{\"0\":4,\"1\":3,\"4294967294\":7,\"b\":1,\"a\":2,\"01\":5,\"4294967295\":6,\"-1\":8}"
    );
    let keys: Vec<&str> = object_entries(value.as_object().unwrap())
        .into_iter()
        .map(|(key, _)| key.as_str())
        .collect();
    assert_eq!(keys, ["0", "1", "4294967294", "b", "a", "01", "4294967295", "-1"]);
}

#[test]
fn numbers_round_trip_as_javascript_doubles() {
    let value = parse("[1e5, -0, 0.1, 1E+2, 1e-7, 123456789012345678901234, 1.0]").unwrap();
    assert_eq!(stringify(&value), "[100000,0,0.1,100,1e-7,1.2345678901234569e+23,1]");
}

#[test]
fn pretty_output_matches_json_stringify_with_two_space_indent() {
    let value = json!({"a": [], "b": {}, "c": [1, {"d": "\u{1}\u{1f}\u{7f}é"}], "e": 1e21, "f": 1.5e-7});
    assert_eq!(
        stringify_pretty(&value),
        "{\n  \"a\": [],\n  \"b\": {},\n  \"c\": [\n    1,\n    {\n      \"d\": \"\\u0001\\u001f\u{7f}é\"\n    }\n  ],\n  \"e\": 1e+21,\n  \"f\": 1.5e-7\n}"
    );
}

#[test]
fn string_escapes_decode_surrogate_pairs() {
    let value = parse("\"\\u00e9\\ud83d\\ude00\\/\\b\\f\\n\\r\\t\"").unwrap();
    assert_eq!(value, json!("é😀/\u{8}\u{c}\n\r\t"));
    assert_eq!(stringify(&value), "\"é😀/\\b\\f\\n\\r\\t\"");
}
