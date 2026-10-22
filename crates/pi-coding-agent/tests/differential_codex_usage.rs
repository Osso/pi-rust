//! Differential tests for the bundled `codex-usage` extension.

#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use bundled::{Harness, HarnessOptions, StubFetch, test_model};
use oracle::TsOracle;
use pi_coding_agent::core::extensions::types::{CustomMessageContent, NotifyType};
use pi_coding_agent::core::extensions::{Extension, ExtensionRuntime, create_extension_runtime};
use pi_js_compat::base64::buffer_to_base64;
use serde_json::{Map, Value, json};

const SOURCE: &str = "packages/coding-agent/extensions/codex-usage/src/index.ts";

fn base64url(bytes: &[u8]) -> String {
    buffer_to_base64(bytes)
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_owned()
}

fn token_with_claims(claims: &Value) -> String {
    format!(
        "header.{}.signature",
        base64url(serde_json::to_string(claims).unwrap().as_bytes())
    )
}

fn notification_value(notifications: &[(String, Option<NotifyType>)]) -> Value {
    Value::Array(
        notifications
            .iter()
            .map(|(message, notify_type)| {
                json!([
                    message,
                    notify_type.map(|notify_type| match notify_type {
                        NotifyType::Info => "info",
                        NotifyType::Warning => "warning",
                        NotifyType::Error => "error",
                    })
                ])
            })
            .collect(),
    )
}

fn uuid_normalized(text: &str) -> String {
    let pattern =
        regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").expect("uuid pattern");
    pattern.replace_all(text, "<uuid>").into_owned()
}

fn auth_for(provider: &str, token: &str) -> Option<Map<String, Value>> {
    json!({(provider): {
        "type": "oauth",
        "refresh": "refresh-token",
        "access": token,
        "expires": 9_999_999_999_999.0
    }})
    .as_object()
    .cloned()
}

async fn load(runtime: &ExtensionRuntime, fetch: Arc<StubFetch>) -> Extension {
    let options = pi_coding_agent::bundled_extensions::codex_usage::CodexUsageOptions {
        fetch: Some(fetch as Arc<dyn pi_ai::request::Fetch>),
    };
    support::extension(runtime, "/repo", "<first-party:codex-usage>", move |pi| {
        pi_coding_agent::bundled_extensions::codex_usage::codex_usage_extension(pi, &options)
            .expect("codex-usage extension")
    })
    .await
}

fn request_value(request: &pi_ai::request::HttpRequest) -> Value {
    let headers: Map<String, Value> = request
        .headers
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), Value::String(value.clone())))
        .collect();
    json!({
        "method": request.method,
        "url": request.url,
        "headers": headers,
        "body": request
            .body
            .as_ref()
            .map(|body| Value::String(uuid_normalized(&String::from_utf8_lossy(body)))),
    })
}

async fn run_command_case(input: &Value) -> Value {
    let responses: Vec<(u16, String, String)> = input
        .get("responses")
        .and_then(Value::as_array)
        .map(|responses| {
            responses
                .iter()
                .map(|response| {
                    (
                        response.get("status").and_then(Value::as_u64).unwrap_or(200) as u16,
                        response
                            .get("statusText")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        response
                            .get("body")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let stub = Arc::new(StubFetch::new(
        responses
            .iter()
            .map(|(status, status_text, body)| (*status, status_text.as_str(), body.as_str()))
            .collect(),
    ));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, Arc::clone(&stub)).await;
    let mut options = HarnessOptions::new("/repo");
    if let Some(model) = input.get("model").filter(|model| !model.is_null()) {
        let mut model_value = test_model(false);
        model_value.api = model
            .get("api")
            .and_then(Value::as_str)
            .unwrap_or("openai-codex-responses")
            .to_owned();
        model_value.provider = model
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("openai-codex")
            .to_owned();
        options.auth = input
            .get("token")
            .filter(|token| !token.is_null())
            .and_then(Value::as_object)
            .and_then(|claims| {
                let token = token_with_claims(&Value::Object(claims.clone()));
                auth_for(&model_value.provider, &token)
            });
        options.model = Some(model_value);
    } else {
        options.auth = input
            .get("token")
            .filter(|token| !token.is_null())
            .and_then(Value::as_object)
            .and_then(|claims| {
                let token = token_with_claims(&Value::Object(claims.clone()));
                auth_for("openai-codex", &token)
            });
    }
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    harness
        .ui
        .confirm_result(input.get("confirm").and_then(Value::as_bool).unwrap_or(false));
    let handler = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .handler
        .clone();
    let error = handler(
        input
            .get("args")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
            .into(),
        harness.command_context(),
    )
    .await
    .err()
    .map(|error| json!(error.message))
    .unwrap_or(Value::Null);

    let messages: Vec<Value> = harness
        .actions
        .messages
        .lock()
        .unwrap()
        .iter()
        .map(|message| {
            let content = match &message.content {
                CustomMessageContent::Text(text) => json!(text),
                CustomMessageContent::Blocks(_) => Value::Null,
            };
            json!([message.custom_type, content, message.display])
        })
        .collect();
    let requests: Vec<Value> = stub.requests.lock().unwrap().iter().map(request_value).collect();
    json!({
        "name": input.get("name").cloned().unwrap_or(Value::Null),
        "error": error,
        "notifications": notification_value(&harness.ui.notifications()),
        "messages": messages,
        "requests": requests,
    })
}

async fn rust_results(cases: &[Value]) -> Vec<Value> {
    use pi_coding_agent::bundled_extensions::codex_usage::{
        CodexUsage, format_codex_usage, map_codex_usage_payload, resolve_codex_account_info,
        resolve_codex_display_email,
    };

    let mut results = Vec::new();
    for case in cases {
        let kind = case.get("kind").and_then(Value::as_str).unwrap_or_default();
        let mut output = Map::new();
        output.insert("name".to_owned(), case.get("name").cloned().unwrap_or(Value::Null));
        match kind {
            "map" => {
                let payload = case.get("payload").cloned().unwrap_or(Value::Null);
                output.insert(
                    "mapped".to_owned(),
                    serde_json::to_value(map_codex_usage_payload(&payload)).expect("usage"),
                );
            }
            "format" => {
                let usage: CodexUsage =
                    serde_json::from_value(case.get("usage").cloned().unwrap_or(Value::Null)).expect("usage");
                output.insert("text".to_owned(), json!(format_codex_usage(&usage)));
            }
            "token" => {
                let token = token_with_claims(case.get("claims").unwrap_or(&Value::Null));
                match resolve_codex_account_info(&JsString::from(&token)) {
                    Ok(info) => {
                        output.insert("account".to_owned(), serde_json::to_value(info).expect("account"));
                    }
                    Err(error) => {
                        output.insert("error".to_owned(), json!(error));
                    }
                }
            }
            "display_email" => {
                let token = token_with_claims(case.get("claims").unwrap_or(&Value::Null));
                match resolve_codex_display_email(
                    &JsString::from(&token),
                    case.get("storedEmail").map(JsValue::from).as_ref(),
                ) {
                    Ok(email) => {
                        output.insert("email".to_owned(), json!(email));
                    }
                    Err(error) => {
                        output.insert("error".to_owned(), json!(error));
                    }
                }
            }
            "completions" => {
                let prefix = case.get("prefix").and_then(Value::as_str).unwrap_or_default();
                let runtime = create_extension_runtime();
                let stub = Arc::new(StubFetch::new(Vec::new()));
                let extension = load(&runtime, stub).await;
                let completions = extension
                    .read()
                    .commands
                    .get(&pi_js_compat::js_json::JsString::from("usage"))
                    .expect("usage command")
                    .get_argument_completions
                    .clone()
                    .expect("completions");
                output.insert(
                    "completions".to_owned(),
                    serde_json::json!(completions(&pi_js_compat::js_json::JsString::from(prefix)).unwrap()),
                );
            }
            "command" => return_value_into_map(&mut output, run_command_case(case).await),
            other => panic!("unknown codex-usage case {other}"),
        }
        results.push(Value::Object(output));
    }
    results
}

fn return_value_into_map(output: &mut Map<String, Value>, value: Value) {
    if let Value::Object(object) = value {
        for (key, value) in object {
            output.insert(key, value);
        }
    }
}

#[tokio::test]
async fn differential_codex_usage() {
    let base_payload = json!({
        "plan_type": "pro",
        "rate_limit": {
            "primary_window": {"used_percent": 42, "limit_window_seconds": 18_000, "reset_at": 1_800_000_000},
            "secondary_window": {"used_percent": 84.5, "limit_window_seconds": 604_800, "reset_at": 1_800_604_800}
        },
        "credits": {"has_credits": true, "unlimited": false, "balance": "9.99"},
        "rate_limit_reset_credits": {"available_count": 3}
    });
    let usage_with_identity = json!({
        "planType": "pro",
        "accountId": "acct_123",
        "email": "user@example.com",
        "resetCreditsAvailable": 3,
        "limits": [{
            "limitId": "codex",
            "limitName": null,
            "primary": {"usedPercent": 42, "windowMinutes": 300, "resetsAt": 1_800_000_000},
            "secondary": {"usedPercent": 84.5, "windowMinutes": 10_080, "resetsAt": 1_800_604_800},
            "credits": {"hasCredits": true, "unlimited": false, "balance": "9.99"}
        }]
    });
    let token_claims = json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"},
        "email": "user@example.com"
    });
    let cases = vec![
        json!({"name": "map-base", "kind": "map", "payload": base_payload}),
        json!({"name": "map-empty", "kind": "map", "payload": {}}),
        json!({"name": "map-additional", "kind": "map", "payload": {
            "plan_type": "team",
            "additional_rate_limits": [{
                "limit_name": "codex_weekly",
                "metered_feature": "codex_weekly",
                "rate_limit": {"primary_window": {"used_percent": 70, "limit_window_seconds": 604_800, "reset_at": 1_800_604_800}}
            }, {
                "limit_name": "",
                "rate_limit": {"primary_window": {"used_percent": 0}}
            }],
            "credits": null
        }}),
        json!({"name": "format-identity", "kind": "format", "usage": usage_with_identity}),
        json!({"name": "format-anonymous", "kind": "format", "usage": {
            "planType": null,
            "accountId": null,
            "email": null,
            "resetCreditsAvailable": 0,
            "limits": [{
                "limitId": "codex",
                "limitName": null,
                "primary": null,
                "secondary": null,
                "credits": {"hasCredits": false, "unlimited": true, "balance": null}
            }, {
                "limitId": "other",
                "limitName": "Other limit",
                "primary": {"usedPercent": 12.34, "windowMinutes": null, "resetsAt": null},
                "secondary": null,
                "credits": null
            }]
        }}),
        json!({"name": "token-account", "kind": "token", "claims": token_claims}),
        json!({"name": "token-profile", "kind": "token", "claims": {
            "https://api.openai.com/auth": {"chatgpt_account_id": "acct_9"},
            "profile": {"email": "profile@example.com"}
        }}),
        json!({"name": "token-missing-account", "kind": "token", "claims": {}}),
        json!({"name": "display-stored", "kind": "display_email", "claims": token_claims, "storedEmail": "stored@example.com"}),
        json!({"name": "display-claim", "kind": "display_email", "claims": token_claims}),
        json!({"name": "completions-re", "kind": "completions", "prefix": "re"}),
        json!({"name": "completions-empty", "kind": "completions", "prefix": ""}),
        json!({"name": "completions-x", "kind": "completions", "prefix": "x"}),
        json!({"name": "command-usage", "kind": "command", "args": "", "token": token_claims, "responses": [
            {"status": 200, "statusText": "", "body": base_payload.to_string()}
        ]}),
        json!({"name": "command-extra-args", "kind": "command", "args": "bogus", "token": token_claims}),
        json!({"name": "command-missing-login", "kind": "command", "args": "", "token": null}),
        json!({"name": "command-failed", "kind": "command", "args": "", "token": token_claims, "responses": [
            {"status": 500, "statusText": "Server Error", "body": "boom"}
        ]}),
        json!({"name": "command-empty-body", "kind": "command", "args": "", "token": token_claims, "responses": [
            {"status": 200, "statusText": "", "body": ""}
        ]}),
        json!({"name": "reset-declined", "kind": "command", "args": "reset", "token": token_claims, "confirm": false, "responses": [
            {"status": 200, "statusText": "", "body": base_payload.to_string()}
        ]}),
        json!({"name": "reset-no-credit", "kind": "command", "args": "reset", "token": token_claims, "confirm": true, "responses": [
            {"status": 200, "statusText": "", "body": json!({
                "plan_type": "pro",
                "rate_limit": {"primary_window": {"used_percent": 10}},
                "rate_limit_reset_credits": {"available_count": 0}
            }).to_string()}
        ]}),
        json!({"name": "reset-success", "kind": "command", "args": "reset", "token": token_claims, "confirm": true, "responses": [
            {"status": 200, "statusText": "", "body": base_payload.to_string()},
            {"status": 200, "statusText": "", "body": "{\"code\":\"reset\",\"windows_reset\":2}"},
            {"status": 200, "statusText": "", "body": base_payload.to_string()}
        ]}),
        json!({"name": "command-gc-provider", "kind": "command", "args": "", "model": {"api": "openai-codex-responses", "provider": "openai-codex-gc"}, "token": token_claims, "responses": [
            {"status": 200, "statusText": "", "body": base_payload.to_string()}
        ]}),
    ];
    let mut ts = TsOracle::spawn("codex-usage-oracle.ts", SOURCE);
    let response = ts.request(&json!({"id": 1, "operation": "cases", "cases": cases}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected: Vec<Value> = serde_json::from_value(response["output"]["results"].clone()).expect("oracle results");
    let actual = rust_results(&cases).await;
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual,
            expected,
            "case {index} ({}) mismatch\nactual:\n{}\nexpected:\n{}",
            cases[index]["name"],
            serde_json::to_string_pretty(actual).unwrap(),
            serde_json::to_string_pretty(expected).unwrap()
        );
    }
}
