use crate::shared_oracle as oracle;

use pi_ai::utils::json_parse::{parse_json_with_repair, parse_streaming_json, repair_json};
use serde_json::{Number, Value, json};

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("json-parse-oracle.ts", "packages/ai/src");
    compare_repair(&mut oracle);
    compare_strict_parsing(&mut oracle);
    compare_streaming_parsing(&mut oracle);
}

fn compare_repair(oracle: &mut oracle::TsOracle) {
    let cases = repair_cases();
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "repair_json",
        "cases": named_cases(&cases),
    }));
    assert_eq!(response["ok"], json!(true), "repair oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("repair results must be an array");
    assert_eq!(results.len(), cases.len());
    for (result, text) in results.iter().zip(cases.iter()) {
        assert_eq!(
            result["repaired"],
            json!(repair_json(text)),
            "repair mismatch for {text:?}"
        );
    }
}

fn compare_strict_parsing(oracle: &mut oracle::TsOracle) {
    let cases = strict_cases();
    let response = oracle.request(&json!({
        "id": 2,
        "operation": "parse_json_with_repair",
        "cases": named_cases(&cases),
    }));
    assert_eq!(response["ok"], json!(true), "strict oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("strict results must be an array");
    assert_eq!(results.len(), cases.len());
    for (result, text) in results.iter().zip(cases.iter()) {
        match parse_json_with_repair(text) {
            Ok(value) => {
                assert_eq!(
                    result["ok"],
                    json!(true),
                    "expected TypeScript success for {text:?}: {result}"
                );
                assert_eq!(
                    normalize_numbers(result["value"].clone()),
                    normalize_numbers(json!(value)),
                    "strict parse mismatch for {text:?}"
                );
            }
            Err(_) => assert_eq!(
                result["ok"],
                json!(false),
                "expected TypeScript failure for {text:?}: {result}"
            ),
        }
    }
}

fn compare_streaming_parsing(oracle: &mut oracle::TsOracle) {
    let cases = streaming_cases();
    let request_cases: Vec<Value> = cases
        .iter()
        .enumerate()
        .map(|(index, text)| json!({"name": index.to_string(), "text": text}))
        .collect();
    let response = oracle.request(&json!({
        "id": 3,
        "operation": "parse_streaming_json",
        "cases": request_cases,
    }));
    assert_eq!(response["ok"], json!(true), "streaming oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("streaming results must be an array");
    assert_eq!(results.len(), cases.len());
    for (index, (result, text)) in results.iter().zip(cases.iter()).enumerate() {
        assert_eq!(
            normalize_numbers(result["value"].clone()),
            normalize_numbers(json!(parse_streaming_json(
                text.as_ref().map(pi_js_compat::js_json::JsString::from).as_ref()
            ))),
            "streaming mismatch for case {index}: {text:?}"
        );
    }
}

fn named_cases(cases: &[String]) -> Vec<Value> {
    cases
        .iter()
        .enumerate()
        .map(|(index, text)| json!({"name": index.to_string(), "text": text}))
        .collect()
}

fn normalize_numbers(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64().and_then(Number::from_f64) {
            Some(float) => Value::Number(float),
            None => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_numbers).collect()),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, item)| (key, normalize_numbers(item)))
                .collect(),
        ),
        other => other,
    }
}

fn repair_cases() -> Vec<String> {
    vec![
        String::new(),
        "   ".to_owned(),
        "{}".to_owned(),
        "plain text".to_owned(),
        "\"simple\"".to_owned(),
        "{\"text\":\"line\nraw\"}".to_owned(),
        "{\"text\":\"tab\there\"}".to_owned(),
        "{\"text\":\"bell\u{7}\"}".to_owned(),
        "{\"text\":\"\u{1f}\"}".to_owned(),
        "{\"a\":\"\\q\"}".to_owned(),
        "\"\\x41\"".to_owned(),
        "\"abc\\".to_owned(),
        "\"\\n\\t\\r\\b\\f\\/\\\\\\\"\"".to_owned(),
        "\"\\u1234\"".to_owned(),
        "\"\\u12\"".to_owned(),
        "\"\\u12xy\"".to_owned(),
        "\"\\uABCD\"".to_owned(),
        "\"\\uabcd\"".to_owned(),
        "{\"nested\":{\"deep\":[\"a\\qb\",\"c\nd\"]}}".to_owned(),
        "{\"emoji\":\"🙈\",\"escaped\":\"\\uD83D\\uDE48\"}".to_owned(),
        "{\"quote\":\"a\\\"b\",\"backslash\":\"a\\\\b\"}".to_owned(),
        "{\"a\":1}\n\t ".to_owned(),
        "[\"\\q\",\n\"ok\"]".to_owned(),
    ]
}

fn strict_cases() -> Vec<String> {
    let mut cases = vec![
        String::new(),
        "   ".to_owned(),
        "null".to_owned(),
        "true".to_owned(),
        "false".to_owned(),
        "1".to_owned(),
        "-1.5e10".to_owned(),
        "01".to_owned(),
        "1.".to_owned(),
        "1e".to_owned(),
        "not json".to_owned(),
        "{\"a\":1}".to_owned(),
        "{\"a\":1}junk".to_owned(),
        "{\"a\":1,}".to_owned(),
        "[1,2,]".to_owned(),
        "\"abc".to_owned(),
        "{\"a\":".to_owned(),
        "{\"a\":\"\\u1234\"}".to_owned(),
        "{\"a\":\"\\u12\"}".to_owned(),
        "{\"dup\":1,\"dup\":2}".to_owned(),
        "{\"nested\":[{\"a\":[true,null,\"x\"]}]}".to_owned(),
        "{\"emoji\":\"🙈\"}".to_owned(),
    ];
    cases.extend(repair_cases());
    cases
}

fn streaming_cases() -> Vec<Option<String>> {
    let mut cases: Vec<Option<String>> = vec![None, Some(String::new()), Some("   ".to_owned())];

    let documents = [
        "{\"a\":1,\"b\":[true,false,null,\"text\"],\"c\":{\"d\":\"e\"}}",
        "[1,2,3,{\"a\":\"b\"},[4,5]]",
        "{\"emoji\":\"🙈\",\"escaped\":\"a\\nb\",\"unicode\":\"\\u1234\"}",
        "{\"text\":\"line\nraw\"}",
        "{\"a\":\"\\q\",\"b\":2}",
        "\"complete string\"",
        "12345",
        "true",
        "null",
    ];
    for document in documents {
        let characters: Vec<char> = document.chars().collect();
        for length in 0..=characters.len() {
            cases.push(Some(characters[..length].iter().collect()));
        }
    }

    for text in [
        "tru",
        "fals",
        "nul",
        "Inf",
        "-Inf",
        "NaN",
        "-",
        "1e",
        "1.",
        "01",
        "{\"a\":",
        "{\"a\":1,",
        "[1,2,",
        "{\"a\":1}junk",
        "{\"a\":\"\\u12\"}",
        "{\"a\":\"\\q\",",
        "\"abc\\",
        "[[[[",
        "{{{{",
        "]}]}",
    ] {
        cases.push(Some(text.to_owned()));
    }

    cases
}
