use pi_ai::utils::json_parse::{parse_json_with_repair, parse_streaming_json, repair_json};
use serde_json::{Value, json};

#[test]
fn repairs_raw_control_characters_inside_strings() {
    assert_eq!(repair_json("{\"text\":\"line\nraw\"}"), "{\"text\":\"line\\nraw\"}");
    assert_eq!(repair_json("{\"text\":\"tab\there\"}"), "{\"text\":\"tab\\there\"}");
    assert_eq!(repair_json("{\"text\":\"bell\u{7}\"}"), "{\"text\":\"bell\\u0007\"}");
    assert_eq!(repair_json("{\"text\":\"\u{1f}\"}"), "{\"text\":\"\\u001f\"}");
}

#[test]
fn repairs_invalid_escapes_by_doubling_the_backslash() {
    assert_eq!(repair_json("{\"a\":\"\\q\"}"), "{\"a\":\"\\\\q\"}");
    assert_eq!(repair_json("\"\\x41\""), "\"\\\\x41\"");
    assert_eq!(repair_json("\"abc\\"), "\"abc\\\\");
}

#[test]
fn preserves_valid_escapes_and_structure_outside_strings() {
    assert_eq!(
        repair_json("\"\\n\\t\\r\\b\\f\\/\\\\\\\"\""),
        "\"\\n\\t\\r\\b\\f\\/\\\\\\\"\""
    );
    assert_eq!(repair_json("\"\\u1234\""), "\"\\u1234\"");
    assert_eq!(repair_json("{\"a\":1}\n\t "), "{\"a\":1}\n\t ");
    assert_eq!(repair_json("\"\\u12\""), "\"\\u12\"");
    assert_eq!(repair_json("\"\\u12xy\""), "\"\\u12xy\"");
}

#[test]
fn parses_valid_json_and_repairs_malformed_strings() {
    assert_eq!(
        parse_json_with_repair("{\"a\":1}").unwrap(),
        pi_js_compat::js_json::JsValue::from(json!({"a": 1}))
    );
    assert_eq!(
        parse_json_with_repair("{\"text\":\"line\nraw\"}").unwrap(),
        pi_js_compat::js_json::JsValue::from(json!({"text": "line\nraw"}))
    );
    assert_eq!(
        parse_json_with_repair("{\"a\":\"\\q\"}").unwrap(),
        pi_js_compat::js_json::JsValue::from(json!({"a": "\\q"}))
    );
    assert!(parse_json_with_repair("{\"a\":").is_err());
    assert!(parse_json_with_repair("not json").is_err());
}

#[test]
fn parses_complete_streamed_json() {
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{}"))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("[]"))),
        pi_js_compat::js_json::JsValue::from(json!([]))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("null"))),
        pi_js_compat::js_json::JsValue::from(Value::Null)
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("true"))),
        pi_js_compat::js_json::JsValue::from(json!(true))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("1"))),
        pi_js_compat::js_json::JsValue::from(json!(1))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":\"\\u1234\"}"))),
        pi_js_compat::js_json::JsValue::from(json!({"a": "ሴ"}))
    );
}

#[test]
fn parses_incomplete_objects_arrays_strings_and_numbers() {
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":1"))),
        pi_js_compat::js_json::JsValue::from(json!({"a": 1}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":"))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":\"hel"))),
        pi_js_compat::js_json::JsValue::from(json!({"a": "hel"}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":1,"))),
        pi_js_compat::js_json::JsValue::from(json!({"a": 1}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("[1,2"))),
        pi_js_compat::js_json::JsValue::from(json!([1, 2]))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("[1,2,"))),
        pi_js_compat::js_json::JsValue::from(json!([1, 2]))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("\"abc"))),
        pi_js_compat::js_json::JsValue::from(json!("abc"))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("1e"))),
        pi_js_compat::js_json::JsValue::from(json!(1))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":1}junk"))),
        pi_js_compat::js_json::JsValue::from(json!({"a": 1}))
    );
}

#[test]
fn accepts_partial_literals_and_maps_nullish_results_to_an_object() {
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("tru"))),
        pi_js_compat::js_json::JsValue::from(json!(true))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("fals"))),
        pi_js_compat::js_json::JsValue::from(json!(false))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("nul"))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("Inf"))),
        pi_js_compat::js_json::JsValue::from(Value::Null)
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("-Inf"))),
        pi_js_compat::js_json::JsValue::from(Value::Null)
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("NaN"))),
        pi_js_compat::js_json::JsValue::from(Value::Null)
    );
}

#[test]
fn returns_an_empty_object_for_missing_blank_or_unparseable_input() {
    assert_eq!(
        parse_streaming_json(None),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from(""))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("   \n\t"))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("-"))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("1."))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("01"))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":\"\\u12\"}"))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
}

#[test]
fn repairs_before_partial_parsing() {
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"a\":\"\\q\","))),
        pi_js_compat::js_json::JsValue::from(json!({}))
    );
    assert_eq!(
        parse_streaming_json(Some(&pi_js_compat::js_json::JsString::from("{\"text\":\"line\n"))),
        pi_js_compat::js_json::JsValue::from(json!({"text": "line"}))
    );
}
