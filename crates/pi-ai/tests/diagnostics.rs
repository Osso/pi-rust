use pi_ai::utils::diagnostics::{
    AssistantMessageDiagnostic, DiagnosticCode, DiagnosticErrorInfo, ThrownValue, append_assistant_message_diagnostic,
    create_assistant_message_diagnostic, extract_diagnostic_error, format_thrown_value,
};
use serde_json::{Value, json};

fn error_info(
    name: Option<&str>,
    message: &str,
    stack: Option<&str>,
    code: Option<DiagnosticCode>,
) -> DiagnosticErrorInfo {
    DiagnosticErrorInfo {
        name: name.map(str::to_owned),
        message: message.to_owned(),
        stack: stack.map(str::to_owned),
        code,
    }
}

#[test]
fn formats_errors_strings_and_json_values_like_javascript_string() {
    let error = ThrownValue::Error(error_info(Some("TypeError"), "boom", None, None));
    assert_eq!(format_thrown_value(&error), "boom");

    let unnamed_error = ThrownValue::Error(error_info(None, "", None, None));
    assert_eq!(format_thrown_value(&unnamed_error), "");

    let named_only = ThrownValue::Error(error_info(Some("AbortError"), "", None, None));
    assert_eq!(format_thrown_value(&named_only), "AbortError");

    assert_eq!(format_thrown_value(&ThrownValue::Text("plain".to_owned())), "plain");
    assert_eq!(format_thrown_value(&ThrownValue::Json(json!("text"))), "text");
    assert_eq!(format_thrown_value(&ThrownValue::Json(json!(null))), "null");
    assert_eq!(format_thrown_value(&ThrownValue::Json(json!(true))), "true");
    assert_eq!(format_thrown_value(&ThrownValue::Json(json!(42))), "42");
    assert_eq!(format_thrown_value(&ThrownValue::Json(json!(-1.5))), "-1.5");
    assert_eq!(format_thrown_value(&ThrownValue::Json(json!([1, "a", null]))), "1,a,");
    assert_eq!(
        format_thrown_value(&ThrownValue::Json(json!({"reason": "boom"}))),
        "[object Object]"
    );
}

#[test]
fn extracts_error_fields_and_keeps_supported_codes() {
    let thrown = ThrownValue::Error(error_info(
        Some("Error"),
        "request failed",
        Some("Error: request failed\n    at handler"),
        Some(DiagnosticCode::Number(429.0)),
    ));

    let extracted = extract_diagnostic_error(&thrown);

    assert_eq!(
        extracted,
        DiagnosticErrorInfo {
            name: Some("Error".to_owned()),
            message: "request failed".to_owned(),
            stack: Some("Error: request failed\n    at handler".to_owned()),
            code: Some(DiagnosticCode::Number(429.0)),
        }
    );
}

#[test]
fn empty_error_name_becomes_none_and_empty_message_falls_back_to_name() {
    let thrown = ThrownValue::Error(error_info(
        Some(""),
        "",
        None,
        Some(DiagnosticCode::String("EPIPE".to_owned())),
    ));

    let extracted = extract_diagnostic_error(&thrown);

    assert_eq!(extracted.name, None);
    assert_eq!(extracted.message, "");
    assert_eq!(extracted.code, Some(DiagnosticCode::String("EPIPE".to_owned())));
}

#[test]
fn non_error_values_are_reported_as_thrown_values() {
    let extracted = extract_diagnostic_error(&ThrownValue::Json(json!({"reason": "boom"})));

    assert_eq!(extracted.name.as_deref(), Some("ThrownValue"));
    assert_eq!(extracted.message, "[object Object]");
    assert_eq!(extracted.stack, None);
    assert_eq!(extracted.code, None);

    let text = extract_diagnostic_error(&ThrownValue::Text("string failure".to_owned()));
    assert_eq!(text.name.as_deref(), Some("ThrownValue"));
    assert_eq!(text.message, "string failure");
}

#[test]
fn creates_a_timestamped_diagnostic_with_optional_details() {
    let error = ThrownValue::Error(error_info(Some("Error"), "boom", None, None));
    let details = serde_json::Map::from_iter([("provider".to_owned(), Value::String("openai".to_owned()))]);

    let diagnostic = create_assistant_message_diagnostic("provider_error", &error, Some(details));

    assert_eq!(diagnostic.r#type, "provider_error");
    assert!(diagnostic.timestamp > 0);
    assert_eq!(diagnostic.error.unwrap().message, "boom");
    assert_eq!(diagnostic.details.unwrap()["provider"], json!("openai"));

    let without_details = create_assistant_message_diagnostic("other", &error, None);
    assert_eq!(without_details.details, None);
}

#[test]
fn appends_diagnostics_into_an_optional_list() {
    let first = AssistantMessageDiagnostic {
        r#type: "first".to_owned(),
        timestamp: 1,
        error: None,
        details: None,
    };
    let second = AssistantMessageDiagnostic {
        r#type: "second".to_owned(),
        timestamp: 2,
        error: None,
        details: None,
    };

    let mut diagnostics = None;
    append_assistant_message_diagnostic(&mut diagnostics, first.clone());
    append_assistant_message_diagnostic(&mut diagnostics, second.clone());

    assert_eq!(diagnostics, Some(vec![first, second]));
}

#[test]
fn serializes_with_typescript_wire_names_and_omits_absent_optionals() {
    let diagnostic = AssistantMessageDiagnostic {
        r#type: "provider_error".to_owned(),
        timestamp: 1_700_000_000_000,
        error: Some(error_info(
            Some("Error"),
            "boom",
            None,
            Some(DiagnosticCode::Number(429.0)),
        )),
        details: None,
    };

    assert_eq!(
        serde_json::to_value(&diagnostic).unwrap(),
        json!({
            "type": "provider_error",
            "timestamp": 1_700_000_000_000_i64,
            "error": {"name": "Error", "message": "boom", "code": 429},
        })
    );

    let bare = AssistantMessageDiagnostic {
        r#type: "bare".to_owned(),
        timestamp: 0,
        error: None,
        details: None,
    };
    assert_eq!(
        serde_json::to_value(&bare).unwrap(),
        json!({"type": "bare", "timestamp": 0})
    );
}

#[test]
fn deserializes_diagnostic_codes_and_optional_fields() {
    let parsed: AssistantMessageDiagnostic = serde_json::from_value(json!({
        "type": "provider_error",
        "timestamp": 12,
        "error": {"message": "boom", "code": "EPIPE"},
        "details": {"attempt": 2},
    }))
    .unwrap();

    assert_eq!(
        parsed.error.unwrap().code,
        Some(DiagnosticCode::String("EPIPE".to_owned()))
    );
    assert_eq!(parsed.details.unwrap()["attempt"], json!(2));

    let numeric: DiagnosticErrorInfo = serde_json::from_value(json!({"message": "boom", "code": 500})).unwrap();
    assert_eq!(numeric.code, Some(DiagnosticCode::Number(500.0)));
}
