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
fn ignores_non_object_response_bodies() {
    let mut error = provider_error("Invocation of model ID is not supported.");
    error.metadata_http_status_code = Some(400.0);
    error.response_body = Some(ErrorBody::Json(json!(["stream", "internals"])));

    let normalized = normalize_provider_error(&ProviderErrorInput::Error(Box::new(error)));

    assert_eq!(normalized.status, Some(400.0));
    assert_eq!(normalized.body, None);
    assert!(normalized.message_carries_body);
}

#[test]
fn ignores_class_like_error_fields_and_empty_objects() {
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
    assert_eq!(truncate_error_text("a🙈b", 2), "a... [truncated 2 chars]");
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
fn returns_the_bare_message_for_a_non_error_value() {
    let normalized = normalize_provider_error(&ProviderErrorInput::Value(json!({"reason": "boom"})));
    assert_eq!(format_provider_error(&normalized, None), "{\"reason\":\"boom\"}");
}
