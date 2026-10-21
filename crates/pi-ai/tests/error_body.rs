use pi_ai::utils::error_body::{
    ErrorBody, MAX_PROVIDER_ERROR_BODY_CHARS, NormalizedProviderError, ProviderError, ProviderErrorInput,
    format_provider_error, normalize_provider_error, safe_json_stringify, truncate_error_text,
};
use serde_json::json;

fn provider_error(message: &str) -> ProviderError {
    ProviderError {
        message: message.to_owned(),
        status_code: None,
        status: None,
        body: None,
        error: None,
        metadata_http_status_code: None,
        response_status_code: None,
        response_body: None,
    }
}

#[test]
fn extracts_status_and_body_from_a_mistral_shaped_error() {
    let mut error = provider_error("Mistral request failed");
    error.status_code = Some(403.0);
    error.body = Some(ErrorBody::Text("{\"error\":\"blocked by gateway WAF\"}".to_owned()));

    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));

    assert_eq!(normalized.status, Some(403.0));
    assert_eq!(
        normalized.body.as_deref(),
        Some("{\"error\":\"blocked by gateway WAF\"}")
    );
    assert!(!normalized.message_carries_body);
}

#[test]
fn reads_the_parsed_body_off_an_openai_shaped_error() {
    let mut error = provider_error("403 status code (no body)");
    error.status = Some(403.0);
    error.error = Some(json!({"error": "blocked by gateway WAF"}));

    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));

    assert_eq!(normalized.status, Some(403.0));
    assert_eq!(
        normalized.body.as_deref(),
        Some("{\"error\":\"blocked by gateway WAF\"}")
    );
    assert!(!normalized.message_carries_body);
}

#[test]
fn preserves_a_message_that_already_carries_the_body() {
    let body = json!({"error": {"code": 403, "message": "Permission denied"}});
    let mut error = provider_error(&serde_json::to_string(&body).unwrap());
    error.status = Some(403.0);
    error.error = Some(body);

    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));

    assert_eq!(normalized.status, Some(403.0));
    assert!(normalized.message_carries_body);
    assert_eq!(
        normalized.body.as_deref(),
        Some("{\"error\":{\"code\":403,\"message\":\"Permission denied\"}}")
    );
}

#[test]
fn extracts_status_and_body_from_a_bedrock_shaped_error() {
    let mut error = provider_error("UnknownError");
    error.metadata_http_status_code = Some(403.0);
    error.response_status_code = Some(403.0);
    error.response_body = Some(ErrorBody::Text("{\"message\":\"blocked by gateway WAF\"}".to_owned()));

    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));

    assert_eq!(normalized.status, Some(403.0));
    assert_eq!(
        normalized.body.as_deref(),
        Some("{\"message\":\"blocked by gateway WAF\"}")
    );
    assert!(!normalized.message_carries_body);
}

#[test]
fn accepts_nonempty_array_response_bodies() {
    let mut error = provider_error("Invocation of model ID is not supported.");
    error.metadata_http_status_code = Some(400.0);
    error.response_body = Some(ErrorBody::Json(json!(["stream", "internals"])));

    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));

    assert_eq!(normalized.status, Some(400.0));
    assert_eq!(normalized.body.as_deref(), Some("[\"stream\",\"internals\"]"));
    assert!(!normalized.message_carries_body);
}

#[test]
fn ignores_empty_arrays_and_objects() {
    let mut error = provider_error("TLS handshake failed");
    error.status = Some(502.0);
    error.error = Some(json!([]));
    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));
    assert_eq!(normalized.body, None);
    assert!(normalized.message_carries_body);

    let mut empty = provider_error("403 status code (no body)");
    empty.status = Some(403.0);
    empty.error = Some(json!({}));
    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(empty)));
    assert_eq!(normalized.body, None);
    assert!(normalized.message_carries_body);
}

#[test]
fn still_surfaces_a_plain_parsed_json_body_object() {
    let mut error = provider_error("400 status code (no body)");
    error.status = Some(400.0);
    error.error = Some(json!({"message": "schema validation failed", "field": "tools[0]"}));

    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));

    assert_eq!(
        normalized.body.as_deref(),
        Some("{\"message\":\"schema validation failed\",\"field\":\"tools[0]\"}")
    );
    assert!(!normalized.message_carries_body);
}

#[test]
fn json_stringifies_a_non_error_thrown_value() {
    let normalized = normalize_provider_error(&ProviderErrorInput::Value(json!({"reason": "boom"})));

    assert_eq!(normalized.status, None);
    assert_eq!(normalized.body, None);
    assert_eq!(normalized.message, "{\"reason\":\"boom\"}");
    assert!(!normalized.message_carries_body);
}

#[test]
fn truncates_the_body_at_the_cap() {
    let long_body = "x".repeat(MAX_PROVIDER_ERROR_BODY_CHARS + 50);
    let mut error = provider_error("failed");
    error.status_code = Some(500.0);
    error.body = Some(ErrorBody::Text(long_body.clone()));

    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));

    assert!(normalized.body.as_deref().unwrap().contains("... [truncated 50 chars]"));
    assert!(normalized.body.as_deref().unwrap().len() < long_body.len());
}

#[test]
fn sets_message_carries_body_when_the_message_contains_the_extracted_body() {
    let mut error = provider_error("500: upstream exploded");
    error.status_code = Some(500.0);
    error.body = Some(ErrorBody::Text("upstream exploded".to_owned()));

    assert!(normalize_provider_error(&ProviderErrorInput::Error(Box::new(error))).message_carries_body);
}

#[test]
fn truncates_by_utf16_length_and_reports_the_removed_count() {
    assert_eq!(truncate_error_text("abc", 3), "abc");
    assert_eq!(truncate_error_text("abcd", 3), "abc... [truncated 1 chars]");
    assert_eq!(truncate_error_text("🙈🙈🙈", 4), "🙈🙈... [truncated 2 chars]");
}

#[test]
fn safe_json_stringify_matches_javascript_output() {
    assert_eq!(safe_json_stringify(&json!({"reason": "boom"})), "{\"reason\":\"boom\"}");
    assert_eq!(safe_json_stringify(&json!("text")), "\"text\"");
    assert_eq!(safe_json_stringify(&json!(null)), "null");
    assert_eq!(safe_json_stringify(&json!([1, true, null])), "[1,true,null]");
}

#[test]
fn formats_status_and_body_with_and_without_a_prefix() {
    let normalized = NormalizedProviderError {
        status: Some(403.0),
        body: Some("{\"error\":\"blocked\"}".to_owned()),
        message: "403 status code (no body)".to_owned(),
        message_carries_body: false,
    };

    assert_eq!(format_provider_error(&normalized, None), "403: {\"error\":\"blocked\"}");
    assert_eq!(
        format_provider_error(&normalized, Some("OpenAI API error")),
        "OpenAI API error (403): {\"error\":\"blocked\"}"
    );
}

#[test]
fn formats_a_message_that_carries_the_body_with_prefix_and_status() {
    let body = "{\"error\":{\"message\":\"Permission denied\"}}";
    let normalized = NormalizedProviderError {
        status: Some(403.0),
        body: Some(body.to_owned()),
        message: body.to_owned(),
        message_carries_body: true,
    };

    assert_eq!(
        format_provider_error(&normalized, Some("OpenAI API error")),
        format!("OpenAI API error (403): {body}")
    );
    assert_eq!(format_provider_error(&normalized, None), body);
}

#[test]
fn body_selection_preserves_first_hit_even_when_empty() {
    for text in ["", " \t\r\n", "\u{feff}\u{a0}"] {
        let error = ProviderError {
            body: Some(ErrorBody::Text(text.to_owned())),
            error: Some(json!(["parsed"])),
            response_body: Some(ErrorBody::Text("response".to_owned())),
            ..provider_error("failed")
        };
        assert_eq!(
            normalize_provider_error(&ProviderErrorInput::Error(Box::new(error))).body,
            None
        );
    }
    let error = ProviderError {
        error: Some(json!(["parsed"])),
        response_body: Some(ErrorBody::Text("response".to_owned())),
        ..provider_error("failed")
    };
    assert_eq!(
        normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)))
            .body
            .as_deref(),
        Some("[\"parsed\"]")
    );
}

#[test]
fn trims_only_ecmascript_whitespace() {
    for (text, expected) in [
        ("\u{feff}body\u{feff}", "body"),
        ("\u{85}body\u{85}", "\u{85}body\u{85}"),
    ] {
        let error = ProviderError {
            body: Some(ErrorBody::Text(text.to_owned())),
            ..provider_error("failed")
        };
        assert_eq!(
            normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)))
                .body
                .as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn json_string_bodies_are_raw_text_in_both_sdk_fields() {
    for response in [false, true] {
        let mut error = provider_error("failed");
        let body = Some(ErrorBody::Json(json!("  raw body  ")));
        if response {
            error.response_body = body;
        } else {
            error.body = body;
        }
        assert_eq!(
            normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)))
                .body
                .as_deref(),
            Some("raw body")
        );
    }
}

#[test]
fn stringify_uses_javascript_key_order_and_binary64_numbers() {
    let value: serde_json::Value =
        serde_json::from_str(r#"{"z":1.0,"10":1e-7,"2":1e20,"01":-0.0,"nested":{"9":9007199254740993,"1":1e21}}"#)
            .unwrap();
    assert_eq!(
        safe_json_stringify(&value),
        r#"{"2":100000000000000000000,"10":1e-7,"z":1,"01":0,"nested":{"1":1e+21,"9":9007199254740992}}"#
    );
}

#[test]
fn first_numeric_status_wins_including_zero_and_nonfinite() {
    for (status_code, status, metadata, response, expected) in [
        (Some(0.0), Some(402.0), Some(403.0), Some(404.0), 0.0),
        (None, Some(402.0), Some(403.0), Some(404.0), 402.0),
        (None, None, Some(403.0), Some(404.0), 403.0),
        (None, None, None, Some(404.0), 404.0),
        (Some(f64::INFINITY), Some(402.0), None, None, f64::INFINITY),
        (Some(f64::NAN), Some(402.0), None, None, f64::NAN),
    ] {
        let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(ProviderError {
            status_code,
            status,
            metadata_http_status_code: metadata,
            response_status_code: response,
            ..provider_error("failed")
        })));
        if expected.is_nan() {
            assert!(normalized.status.unwrap().is_nan());
        } else {
            assert_eq!(normalized.status, Some(expected));
        }
    }
}

#[test]
fn formatting_covers_independent_optional_fields_and_empty_prefix() {
    for status in [None, Some(-0.0), Some(f64::INFINITY), Some(f64::NAN)] {
        for body in [None, Some("body".to_owned())] {
            for carries in [false, true] {
                for prefix in [None, Some(""), Some("Provider")] {
                    let normalized = NormalizedProviderError {
                        status,
                        body: body.clone(),
                        message: "message".to_owned(),
                        message_carries_body: carries,
                    };
                    let detail = if !carries && status.is_some() && body.is_some() {
                        "body"
                    } else {
                        "message"
                    };
                    let status_text = status.map(|number| {
                        if number.is_nan() {
                            "NaN"
                        } else if number.is_infinite() {
                            "Infinity"
                        } else {
                            "0"
                        }
                    });
                    let expected = match (prefix, status_text) {
                        (Some(prefix), Some(status)) => format!("{prefix} ({status}): {detail}"),
                        (None, Some(status)) if detail == "body" => format!("{status}: body"),
                        _ => "message".to_owned(),
                    };
                    assert_eq!(format_provider_error(&normalized, prefix), expected);
                }
            }
        }
    }
}

#[test]
fn returns_the_bare_message_for_a_non_error_value() {
    let normalized = normalize_provider_error(&ProviderErrorInput::Value(json!({"reason": "boom"})));
    assert_eq!(format_provider_error(&normalized, None), "{\"reason\":\"boom\"}");
}
