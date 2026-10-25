use crate::shared_oracle as oracle;

use pi_ai::utils::error_body::{
    ErrorBody, NormalizedProviderError, ProviderError, ProviderErrorInput, format_provider_error,
    normalize_provider_error, truncate_error_text,
};
use serde_json::{Value, json};

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("error-body-oracle.ts", "packages/ai/src");
    compare_errors(&mut oracle);
    compare_truncation(&mut oracle);
}

fn compare_errors(oracle: &mut oracle::TsOracle) {
    compare_error_cases(oracle, error_cases());
}

fn compare_error_cases(oracle: &mut oracle::TsOracle, cases: Vec<ErrorCase>) {
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "normalize",
        "cases": cases.iter().enumerate().map(|(index, case)| case.request(index)).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], json!(true), "normalize oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("normalize results must be an array");
    assert_eq!(results.len(), cases.len());
    for (result, case) in results.iter().zip(cases.iter()) {
        let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(case.error.clone())));
        assert_eq!(
            result["status"].as_f64(),
            normalized.status,
            "status mismatch in {result}"
        );
        assert_eq!(
            result["body"],
            normalized.body.as_ref().map_or(Value::Null, |body| json!(body))
        );
        assert_eq!(result["message"], json!(normalized.message));
        assert_eq!(result["messageCarriesBody"], json!(normalized.message_carries_body));
        assert_eq!(
            result["formattedWithoutPrefix"],
            json!(format_provider_error(&normalized, None))
        );
        assert_eq!(
            result["formatted"],
            json!(format_provider_error(&normalized, case.prefix.as_deref()))
        );
    }
}

fn compare_values(oracle: &mut oracle::TsOracle) {
    let values = [
        json!("boom"),
        json!(42),
        json!(null),
        json!(true),
        json!({"reason": "boom"}),
        json!({"nested": {"a": [1, 2]}}),
        json!([1, 2, 3]),
        serde_json::from_str(r#"{"z":1.0,"10":1e-7,"2":1e20,"01":-0.0,"nested":{"9":9007199254740993,"1":1e21}}"#)
            .unwrap(),
    ];
    let response = oracle.request(&json!({
        "id": 2,
        "operation": "normalize_value",
        "cases": values.iter().enumerate().map(|(index, value)| json!({"name": index.to_string(), "value": value, "prefix": "Provider"})).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], json!(true), "value oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("value results must be an array");
    assert_eq!(results.len(), values.len());
    for (result, value) in results.iter().zip(values.iter()) {
        let normalized = normalize_provider_error(&ProviderErrorInput::Value(pi_js_compat::js_json::JsValue::from(
            value.clone(),
        )));
        assert_eq!(result["message"], json!(normalized.message));
        assert_eq!(
            result["formatted"],
            json!(format_provider_error(&normalized, Some("Provider")))
        );
        assert_eq!(
            result["formattedWithoutPrefix"],
            json!(format_provider_error(&normalized, None))
        );
    }
}

fn compare_truncation(oracle: &mut oracle::TsOracle) {
    let cases = truncation_cases();
    let response = oracle.request(&json!({
        "id": 3,
        "operation": "truncate",
        "cases": cases.iter().enumerate().map(|(index, (text, max_chars))| json!({"name": index.to_string(), "text": text, "maxChars": max_chars})).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], json!(true), "truncate oracle failed: {response}");

    let results = response["output"]["results"]
        .as_array()
        .expect("truncate results must be an array");
    assert_eq!(results.len(), cases.len());
    for (result, (text, max_chars)) in results.iter().zip(cases.iter()) {
        assert_eq!(
            result["units"],
            json!(
                truncate_error_text(&pi_js_compat::js_json::JsString::from(text), *max_chars)
                    .encode_utf16()
                    .collect::<Vec<_>>()
            ),
            "truncate mismatch for {text:?}"
        );
    }
}

struct ErrorCase {
    error: ProviderError,
    prefix: Option<String>,
}

impl ErrorCase {
    fn request(&self, index: usize) -> Value {
        let error = &self.error;
        json!({
            "name": index.to_string(),
            "message": error.message,
            "statusCode": error.status_code,
            "status": error.status,
            "body": error.body.as_ref().map(body_value),
            "error": error.error,
            "metadataHttpStatusCode": error.metadata_http_status_code,
            "responseStatusCode": error.response_status_code,
            "responseBody": error.response_body.as_ref().map(body_value),
            "prefix": self.prefix,
        })
    }
}

fn body_value(body: &ErrorBody) -> Value {
    match body {
        ErrorBody::Text(text) => json!(text),
        ErrorBody::Json(value) => json!(value),
    }
}

fn case(error: ProviderError, prefix: Option<&str>) -> ErrorCase {
    ErrorCase {
        error,
        prefix: prefix.map(str::to_owned),
    }
}

fn base(message: &str) -> ProviderError {
    ProviderError {
        message: (message.to_owned()).into(),
        ..ProviderError::default()
    }
}

fn error_cases() -> Vec<ErrorCase> {
    let mut cases = vec![
        case(base("plain failure"), None),
        case(base("plain failure"), Some("Provider")),
        case(
            ProviderError {
                status_code: Some(403.0),
                body: Some(ErrorBody::Text(
                    ("{\"error\":\"blocked by gateway WAF\"}".to_owned()).into(),
                )),
                ..base("Mistral request failed")
            },
            Some("Mistral API error"),
        ),
        case(
            ProviderError {
                status: Some(403.0),
                error: Some(pi_js_compat::js_json::JsValue::from(
                    json!({"error": "blocked by gateway WAF"}),
                )),
                ..base("403 status code (no body)")
            },
            None,
        ),
        case(
            ProviderError {
                status: Some(403.0),
                error: Some(pi_js_compat::js_json::JsValue::from(
                    json!({"error": {"code": 403, "message": "Permission denied"}}),
                )),
                ..base("{\"error\":{\"code\":403,\"message\":\"Permission denied\"}}")
            },
            Some("OpenAI API error"),
        ),
        case(
            ProviderError {
                metadata_http_status_code: Some(403.0),
                response_status_code: Some(403.0),
                response_body: Some(ErrorBody::Text(
                    ("{\"message\":\"blocked by gateway WAF\"}".to_owned()).into(),
                )),
                ..base("UnknownError")
            },
            None,
        ),
        case(
            ProviderError {
                metadata_http_status_code: Some(400.0),
                response_status_code: Some(400.0),
                response_body: Some(ErrorBody::Json(pi_js_compat::js_json::JsValue::from(json!([
                    "stream",
                    "internals"
                ])))),
                ..base("Invocation of model ID is not supported.")
            },
            None,
        ),
        case(
            ProviderError {
                response_body: Some(ErrorBody::Json(pi_js_compat::js_json::JsValue::from(
                    json!({"message": "schema validation failed"}),
                ))),
                ..base("ValidationException")
            },
            None,
        ),
        case(
            ProviderError {
                status: Some(502.0),
                error: Some(pi_js_compat::js_json::JsValue::from(json!([]))),
                ..base("TLS handshake failed")
            },
            None,
        ),
        case(
            ProviderError {
                status: Some(403.0),
                error: Some(pi_js_compat::js_json::JsValue::from(json!({}))),
                ..base("403 status code (no body)")
            },
            None,
        ),
        case(
            ProviderError {
                status_code: Some(500.0),
                body: Some(ErrorBody::Text(("upstream exploded".to_owned()).into())),
                ..base("500: upstream exploded")
            },
            None,
        ),
        case(
            ProviderError {
                status_code: Some(500.0),
                status: Some(503.0),
                metadata_http_status_code: Some(504.0),
                response_status_code: Some(505.0),
                body: Some(ErrorBody::Text(("  padded body  ".to_owned()).into())),
                ..base("failed")
            },
            None,
        ),
        case(
            ProviderError {
                status_code: Some(429.5),
                body: Some(ErrorBody::Text(("rate limited".to_owned()).into())),
                ..base("failed")
            },
            Some("Provider"),
        ),
        case(
            ProviderError {
                body: Some(ErrorBody::Text(("   ".to_owned()).into())),
                ..base("failed")
            },
            None,
        ),
        case(
            ProviderError {
                body: Some(ErrorBody::Text(("x".repeat(4050)).into())),
                ..base("failed")
            },
            None,
        ),
        case(
            ProviderError {
                status_code: Some(0.0),
                body: Some(ErrorBody::Text(("zero status".to_owned()).into())),
                ..base("failed")
            },
            None,
        ),
    ];

    cases.push(case(
        ProviderError {
            status_code: Some(403.0),
            body: Some(ErrorBody::Text(("   trimmed body   ".to_owned()).into())),
            ..base("contains trimmed body")
        },
        None,
    ));
    cases
}

pub(crate) fn run_trim() {
    let mut oracle = oracle::TsOracle::spawn("error-body-oracle.ts", "packages/ai/src");
    let whitespace = [
        "\u{feff}", "\u{85}", "\u{a0}", "\u{2000}", "\u{2028}", "\u{2029}", "\u{202f}", "\u{205f}", "\u{3000}",
        "\u{180e}", "\u{200b}",
    ];
    compare_error_cases(
        &mut oracle,
        whitespace
            .into_iter()
            .map(|space| {
                case(
                    ProviderError {
                        body: Some(ErrorBody::Text((format!("{space}body{space}")).into())),
                        ..base("failed")
                    },
                    None,
                )
            })
            .collect(),
    );
}

pub(crate) fn run_stringify() {
    let mut oracle = oracle::TsOracle::spawn("error-body-oracle.ts", "packages/ai/src");
    compare_values(&mut oracle);
}

pub(crate) fn run_selection() {
    let mut oracle = oracle::TsOracle::spawn("error-body-oracle.ts", "packages/ai/src");
    let mut cases = Vec::new();
    for value in [
        json!(null),
        json!(false),
        json!(0),
        json!("ignored"),
        json!([]),
        json!({}),
        json!([null]),
        json!({"2": "two", "1": "one"}),
    ] {
        cases.push(case(
            ProviderError {
                body: Some(ErrorBody::Json(pi_js_compat::js_json::JsValue::from(value.clone()))),
                error: Some(pi_js_compat::js_json::JsValue::from(json!(["parsed"]))),
                response_body: Some(ErrorBody::Text(("response".to_owned()).into())),
                ..base("failed")
            },
            Some(""),
        ));
        cases.push(case(
            ProviderError {
                error: Some(pi_js_compat::js_json::JsValue::from(value.clone())),
                response_body: Some(ErrorBody::Text(("response".to_owned()).into())),
                ..base("failed")
            },
            Some("Provider"),
        ));
        cases.push(case(
            ProviderError {
                response_body: Some(ErrorBody::Json(pi_js_compat::js_json::JsValue::from(value))),
                ..base("failed")
            },
            None,
        ));
    }
    for text in ["", " \t\r\n", "\u{feff}", " raw body "] {
        for body in [
            ErrorBody::Text((text.to_owned()).into()),
            ErrorBody::Json(pi_js_compat::js_json::JsValue::from(json!(text))),
        ] {
            cases.push(case(
                ProviderError {
                    body: Some(body.clone()),
                    error: Some(pi_js_compat::js_json::JsValue::from(json!(["parsed"]))),
                    response_body: Some(ErrorBody::Text(("response".to_owned()).into())),
                    ..base("failed")
                },
                None,
            ));
            cases.push(case(
                ProviderError {
                    response_body: Some(body),
                    ..base("failed")
                },
                None,
            ));
        }
    }
    for (status_code, status, metadata_http_status_code, response_status_code) in [
        (Some(0.0), Some(402.0), Some(403.0), Some(404.0)),
        (None, Some(402.0), Some(403.0), Some(404.0)),
        (None, None, Some(403.0), Some(404.0)),
        (None, None, None, Some(404.0)),
    ] {
        cases.push(case(
            ProviderError {
                status_code,
                status,
                metadata_http_status_code,
                response_status_code,
                body: Some(ErrorBody::Text(("body".to_owned()).into())),
                ..base("failed")
            },
            Some(""),
        ));
    }
    compare_error_cases(&mut oracle, cases);
}

pub(crate) fn run_format() {
    let mut oracle = oracle::TsOracle::spawn("error-body-oracle.ts", "packages/ai/src");
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for status in [
        None,
        Some(-0.0),
        Some(429.5),
        Some(f64::NAN),
        Some(f64::INFINITY),
        Some(f64::NEG_INFINITY),
    ] {
        for body in [None, Some(""), Some("body")] {
            for carries in [false, true] {
                for prefix in [None, Some(""), Some("Provider")] {
                    let normalized = NormalizedProviderError {
                        status,
                        body: (body.map(str::to_owned)).map(pi_js_compat::js_json::JsString::from),
                        message: ("message".to_owned()).into(),
                        message_carries_body: carries,
                    };
                    let encoded_status = match status {
                        Some(number) if number.is_nan() => json!("NaN"),
                        Some(f64::INFINITY) => json!("Infinity"),
                        Some(f64::NEG_INFINITY) => json!("-Infinity"),
                        _ => json!(status),
                    };
                    let mut request = json!({ "message": "message", "messageCarriesBody": carries });
                    // Absence is undefined in TS, not JSON null; preserve it on the wire.
                    if status.is_some() {
                        request["status"] = encoded_status;
                    }
                    if let Some(body) = body {
                        request["body"] = json!(body);
                    }
                    if let Some(prefix) = prefix {
                        request["prefix"] = json!(prefix);
                    }
                    cases.push(request);
                    expected.push(format_provider_error(&normalized, prefix));
                }
            }
        }
    }
    let response = oracle.request(&json!({ "id": 5, "operation": "format", "cases": cases }));
    assert_eq!(response["ok"], true);
    assert_eq!(response["output"]["results"], json!(expected));
}

pub(crate) fn run_utf16_boundary() {
    let mut oracle = oracle::TsOracle::spawn("error-body-oracle.ts", "packages/ai/src");
    let response = oracle.request(&json!({"id": 4, "operation": "truncate", "cases": [
        {"name": "split", "text": "a🙈b", "maxChars": 2},
        {"name": "body cap split", "text": format!("{}🙈b", "a".repeat(3999)), "maxChars": 4000}
    ]}));
    assert_eq!(response["ok"], true);
    for (index, (text, cap)) in [("a🙈b".to_owned(), 2), (format!("{}🙈b", "a".repeat(3999)), 4000)]
        .iter()
        .enumerate()
    {
        let source_units: Vec<u16> =
            serde_json::from_value(response["output"]["results"][index]["units"].clone()).unwrap();
        let mut expected: Vec<u16> = text.encode_utf16().take(*cap).collect();
        expected.extend("... [truncated 2 chars]".encode_utf16());
        assert_eq!(source_units, expected);
        assert!(
            String::from_utf16(&source_units).is_err(),
            "source returns a lone high surrogate"
        );
        assert_eq!(
            source_units,
            truncate_error_text(&pi_js_compat::js_json::JsString::from(text), *cap)
                .encode_utf16()
                .collect::<Vec<_>>(),
            "native truncation preserves the exact source UTF-16 units"
        );
    }
    let text = format!("{}🙈b", "a".repeat(3999));
    let response = oracle.request(&json!({"id": 6, "operation": "normalize_utf16", "cases": [
        {"name": "body cap", "message": "failed", "body": text}
    ]}));
    assert_eq!(response["ok"], true);
    let source_units: Vec<u16> = serde_json::from_value(response["output"]["results"][0]["bodyUnits"].clone()).unwrap();
    let mut expected: Vec<u16> = text.encode_utf16().take(4000).collect();
    expected.extend("... [truncated 2 chars]".encode_utf16());
    assert_eq!(source_units, expected);
    let native = normalize_provider_error(&ProviderErrorInput::Error(Box::new(ProviderError {
        body: Some(ErrorBody::Text((text).into())),
        ..base("failed")
    })));
    assert_eq!(
        source_units,
        native.body.unwrap().encode_utf16().collect::<Vec<_>>(),
        "native normalization preserves the exact source UTF-16 units"
    );
}

fn truncation_cases() -> Vec<(String, usize)> {
    vec![
        (String::new(), 0),
        ("abc".to_owned(), 3),
        ("abcd".to_owned(), 3),
        ("abcd".to_owned(), 0),
        ("x".repeat(4000), 4000),
        ("x".repeat(4001), 4000),
        ("é🙈abc".to_owned(), 4),
        ("é🙈abc".to_owned(), 5),
        ("line\nbreak".to_owned(), 5),
    ]
}
