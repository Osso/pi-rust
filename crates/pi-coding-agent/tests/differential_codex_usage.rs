//! Differential tests for the bundled `codex-usage` extension.

#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_js_compat::js_json::{JsString, JsValue};
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

fn notification_value(notifications: &[(JsString, Option<NotifyType>)]) -> Value {
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

fn auth_for(provider: &str, token: &str) -> Option<pi_js_compat::js_json::JsObject> {
    JsValue::from(json!({(provider): {
        "type": "oauth",
        "refresh": "refresh-token",
        "access": token,
        "expires": 9_999_999_999_999.0
    }}))
    .as_object()
    .cloned()
}

async fn load(runtime: &ExtensionRuntime, fetch: Arc<StubFetch>, bridge: Option<JsValue>) -> Extension {
    let options = pi_coding_agent::bundled_extensions::codex_usage::CodexUsageOptions {
        fetch: Some(fetch as Arc<dyn pi_ai::request::Fetch>),
    };
    support::extension(runtime, "/repo", "<first-party:codex-usage>", move |pi| {
        if let Some(bridge) = bridge {
            let accounts = bridge.clone();
            pi.events
                .on_request(
                    &"claude-bridge:usage-all-request".into(),
                    Arc::new(move |request| {
                        let accounts = accounts.clone();
                        request.response = Some(Box::pin(async move {
                            tokio::task::yield_now().await;
                            if let Some(error) = accounts.get("error").and_then(JsValue::as_string) {
                                return Err(error.clone());
                            }
                            Ok(accounts.get("accounts").cloned().unwrap_or(JsValue::Null))
                        }));
                        Ok(())
                    }),
                )
                .unwrap();
            pi.events
                .on_request(
                    &"claude-bridge:usage-request".into(),
                    Arc::new(move |request| {
                        if bridge.get("handled").and_then(JsValue::as_bool) != Some(true) {
                            return Ok(());
                        }
                        let ctx = request
                            .context
                            .as_ref()
                            .unwrap()
                            .downcast_ref::<pi_coding_agent::core::extensions::ExtensionCommandContext>()
                            .unwrap()
                            .clone();
                        let args = request.data.get("args").and_then(JsValue::as_string).unwrap().clone();
                        let error = bridge.get("handledError").and_then(JsValue::as_string).cloned();
                        request.response = Some(Box::pin(async move {
                            tokio::task::yield_now().await;
                            if let Some(error) = error {
                                return Err(error);
                            }
                            ctx.ui()
                                .unwrap()
                                .notify(&(JsString::from("forwarded:") + args), Some(NotifyType::Info))
                                .expect("fixture forwarded notification");
                            Ok(JsValue::Null)
                        }));
                        Ok(())
                    }),
                )
                .unwrap();
        }
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
    let extension = load(&runtime, Arc::clone(&stub), input.get("bridge").map(JsValue::from)).await;
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
    let provider = input
        .get("model")
        .and_then(|model| model.get("provider"))
        .and_then(Value::as_str)
        .unwrap_or("openai-codex");
    if let Some(stored_email) = input.get("storedEmail") {
        options
            .auth
            .as_mut()
            .unwrap()
            .get_mut(&JsString::from(provider))
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("email".into(), JsValue::from(stored_email));
    }
    let runtime_token = if input.get("runtimeOnly").and_then(Value::as_bool) == Some(true) {
        options.auth = None;
        Some(token_with_claims(input.get("token").unwrap()))
    } else {
        None
    };
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    if let Some(token) = runtime_token {
        harness
            .command_context()
            .model_registry()
            .unwrap()
            .auth_storage()
            .set_runtime_api_key(provider, &token.into());
    }
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
                    serde_json::to_value(map_codex_usage_payload(&JsValue::from(payload))).expect("usage"),
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
                let extension = load(&runtime, stub, None).await;
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
        json!({"name": "map-missing-percent", "kind": "map", "payload": {"rate_limit":{"primary_window":{"limit_window_seconds":18000}}}}),
        json!({"name":"map-empty-metered-feature", "kind":"map", "payload":{"additional_rate_limits":[{"metered_feature":""}]}}),
        json!({"name":"format-empty-label-balance", "kind":"format", "usage":{"planType":"", "accountId":"", "email":"", "resetCreditsAvailable":null, "limits":[{"limitId":"codex", "limitName":"", "primary":null, "secondary":null, "credits":{"hasCredits":false,"unlimited":false,"balance":""}}]}}),
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

#[tokio::test]
async fn differential_claude_usage_bridge() {
    let mut cases = vec![
        json!({"name":"all-unclaimed", "kind":"command", "args":" all ", "token":null}),
        json!({"name":"all-accounts", "kind":"command", "args":"all", "token":null, "bridge":{"accounts":[
            {"account":"alice", "selected":true, "fiveHour":{"percent":42.25,"resetsAt":"2026-07-01T12:30:00Z"},"weekly":{"percent":101,"resetsAt":"2026-12-01T12:30:00Z"}},
            {"account":"bob\nsk-ant-secret Bearer token eyJabc.def.ghi", "selected":false, "error":"failed\tBearer secret"},
            {"account":"empty", "selected":false,"fiveHour":null,"weekly":null}
        ]}}),
        json!({"name":"all-empty", "kind":"command", "args":"all", "token":null,"bridge":{"accounts":[]}}),
        json!({"name":"all-malformed", "kind":"command", "args":"all", "token":null,"bridge":{"accounts":{}}}),
        json!({"name":"all-rejected", "kind":"command", "args":"all", "token":null,"bridge":{"error":"oops\nBearer abc sk-secret eyJabc.def.ghi"}}),
        json!({"name":"all-malformed-entries", "kind":"command", "args":"all", "token":null,"bridge":{"accounts":[null,[],{}, {"account":" ","selected":false,"error":"x"}, {"account":"bad","selected":true,"fiveHour":{"percent":-1,"resetsAt":null},"weekly":null}, {"account":"bad-date","selected":false,"fiveHour":{"percent":1,"resetsAt":"2026-13-01T00:00:00Z"},"weekly":null}]}}),
        json!({"name":"all-long-control", "kind":"command", "args":"all", "token":null,"bridge":{"accounts":[{"account":"x".repeat(301),"selected":true,"error":"e\u{7f}\u{85}\u{1f}"}]}}),
    ];
    for date in [
        "0000-01-01T00:00:00Z",
        "0001-01-01T00:00:00Z",
        "0099-07-01T00:00:00Z",
        "2026-02-30T00:00:00.123456Z",
    ] {
        cases.push(
            json!({"name":format!("all-date-{date}"),"kind":"command","args":"all","token":null,"bridge":{"accounts":[{
                "account":"date", "selected":false,"fiveHour":{"percent":0,"resetsAt":date},"weekly":null
            }]}}),
        );
    }
    for (name, args, bridge) in [
        ("claude-unclaimed", "", Value::Null),
        ("claude-claimed", " reset ", json!({"handled":true})),
        ("claude-bogus", "bogus", json!({"handled":true})),
        (
            "claude-error",
            "",
            json!({"handled":true,"handledError":"bridge failed"}),
        ),
        ("claude-all-first", "all", json!({"handled":true,"accounts":[]})),
    ] {
        let mut case =
            json!({"name":name,"kind":"command","args":args,"model":{"provider":"claude-bridge"},"token":null});
        if !bridge.is_null() {
            case["bridge"] = bridge;
        }
        cases.push(case);
    }
    let mut ts = TsOracle::spawn("codex-usage-oracle.ts", SOURCE);
    let response = ts.request(&json!({"id":2,"operation":"cases","cases":cases}));
    assert_eq!(response["ok"], true, "{response}");
    let expected = response["output"]["results"].as_array().unwrap();
    let actual = rust_results(&cases).await;
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual, expected, "bridge case {}", actual["name"]);
    }
    assert_eq!(actual.len(), expected.len());
}

async fn run_native_bridge_command(input: &JsValue) -> JsValue {
    let runtime = create_extension_runtime();
    let fetch = Arc::new(StubFetch::new(Vec::new()));
    let extension = load(&runtime, fetch.clone(), input.get("bridge").cloned()).await;
    let mut options = HarnessOptions::new("/repo");
    if let Some(provider) = input
        .get("model")
        .and_then(|model| model.get("provider"))
        .and_then(JsValue::as_string)
    {
        let mut model = test_model(false);
        model.provider = provider.to_utf8().unwrap();
        options.model = Some(model);
    }
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let handler = extension
        .read()
        .commands
        .get(&JsString::from("usage"))
        .unwrap()
        .handler
        .clone();
    let error = handler(
        input.get("args").and_then(JsValue::as_string).unwrap().clone(),
        harness.command_context(),
    )
    .await
    .err();
    let notifications = harness
        .ui
        .notifications()
        .into_iter()
        .map(|(text, kind)| {
            assert_eq!(kind, Some(NotifyType::Info));
            pi_js_compat::js_json!([text, "info"])
        })
        .collect::<Vec<_>>();
    let messages = harness
        .actions
        .messages
        .lock()
        .unwrap()
        .iter()
        .map(|message| {
            let CustomMessageContent::Text(content) = &message.content else {
                panic!("text report")
            };
            pi_js_compat::js_json!([message.custom_type, content, message.display])
        })
        .collect::<Vec<_>>();
    assert!(fetch.requests.lock().unwrap().is_empty());
    pi_js_compat::js_json!({
        "name": input.get("name").unwrap(),
        "error": error.map(|error| error.message),
        "notifications": notifications,
        "messages": messages,
        "requests": [],
    })
}

#[tokio::test]
async fn differential_claude_bridge_raw_utf16() {
    let mut raw_accounts = pi_js_compat::js_json::parse_lossless(r#"{"name":"raw-accounts","kind":"command","args":"all","token":null,"bridge":{"accounts":[{"account":"a\ud800b\udfff","selected":true,"error":"e\udc00\nBearer secret"}]}}"#).unwrap();
    let clipped = JsString::from("a".repeat(299)) + JsString::from_utf16(vec![0xd83d, 0xde00]);
    raw_accounts
        .get_mut("bridge")
        .unwrap()
        .get_mut("accounts")
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(pi_js_compat::js_json!({"account": clipped, "selected": false, "error": clipped}));
    let cases = vec![
        raw_accounts,
        pi_js_compat::js_json::parse_lossless(r#"{"name":"raw-rejected","kind":"command","args":"all","token":null,"bridge":{"error":"e\ud800 Bearer secret"}}"#).unwrap(),
        pi_js_compat::js_json::parse_lossless(r#"{"name":"raw-forwarded","kind":"command","args":" \ud800\udfff ","token":null,"model":{"provider":"claude-bridge"},"bridge":{"handled":true}}"#).unwrap(),
        pi_js_compat::js_json::parse_lossless(r#"{"name":"raw-handled-error","kind":"command","args":"","token":null,"model":{"provider":"claude-bridge"},"bridge":{"handled":true,"handledError":"failed\ud800"}}"#).unwrap(),
    ];
    let mut ts = TsOracle::spawn("codex-usage-oracle.ts", SOURCE);
    let response: JsValue = ts.request(&pi_js_compat::js_json!({"id":3,"operation":"cases","cases": cases}));
    assert_eq!(
        response.get("ok").and_then(JsValue::as_bool),
        Some(true),
        "{response:?}"
    );
    let expected = response
        .get("output")
        .unwrap()
        .get("results")
        .unwrap()
        .as_array()
        .unwrap();
    for (input, expected) in cases.iter().zip(expected) {
        assert_eq!(
            &run_native_bridge_command(input).await,
            expected,
            "{:?}",
            input.get("name")
        );
    }
    assert_eq!(expected.len(), cases.len());
}

#[tokio::test]
async fn differential_usage_all_codex_credentials() {
    let token = json!({"https://api.openai.com/auth":{"chatgpt_account_id":"acct_123"},"email":"claim@example.com"});
    let payload = json!({"plan_type":"pro\nBearer plan", "rate_limit": {
        "primary_window":{"used_percent":12.5,"limit_window_seconds":18000,"reset_at":1782909000},
        "secondary_window":{"used_percent":95,"limit_window_seconds":604800,"reset_at":1796128200}
    },"rate_limit_reset_credits":{"available_count":2}});
    let mut cases = Vec::new();
    for (name, provider, email, runtime_only) in [
        (
            "all-codex-stored-email",
            "openai-codex",
            json!("stored\nsk-secret@example.com"),
            false,
        ),
        ("all-codex-gc", "openai-codex-gc", json!(""), false),
        ("all-codex-team", "openai-codex-team", Value::Null, false),
        ("all-codex-runtime-only", "openai-codex", Value::Null, true),
    ] {
        cases.push(json!({"name":name,"kind":"command","args":"all","token":token,
            "model":{"provider":provider},"storedEmail":email,"runtimeOnly":runtime_only,
            "responses":[{"status":200,"body":payload.to_string()}]}));
    }
    let access = token_with_claims(&token);
    cases.push(
        json!({"name":"all-codex-error-redaction","kind":"command","args":"all","token":token,
        "responses":[{"status":500,"body":format!("bad {access} twice {access}\nBearer secret sk-secret")}]}),
    );
    let mut ts = TsOracle::spawn("codex-usage-oracle.ts", SOURCE);
    let response = ts.request(&json!({"id":4,"operation":"cases","cases":cases}));
    assert_eq!(response["ok"], true, "{response}");
    let expected = response["output"]["results"].as_array().unwrap();
    let actual = rust_results(&cases).await;
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual, expected, "credential case {}", actual["name"]);
    }
    assert_eq!(actual.len(), expected.len());
}

#[test]
fn usage_human_fields_match_frozen_utf16_in_both_directions() {
    use pi_coding_agent::bundled_extensions::codex_usage::{CodexUsage, format_codex_usage, map_codex_usage_payload};
    use pi_js_compat::{js_json, js_json as native_json};
    let payload = js_json::parse_lossless(r#"{"plan_type":"plan\ud800", "credits":{"balance":"10\udfff"}, "additional_rate_limits":[{"limit_name":"rate\ud801", "metered_feature":"feature\ud802"}]}"#).unwrap();
    let mut ts = TsOracle::spawn("codex-usage-oracle.ts", SOURCE);
    let response: JsValue = ts.request(
        &native_json!({"id":91,"operation":"cases","cases":[{"name":"raw-map","kind":"map","payload":payload}]}),
    );
    assert_eq!(response["ok"], native_json!(true));
    let expected = &response["output"]["results"][0]["mapped"];
    let native = map_codex_usage_payload(&payload);
    assert_eq!(js_json::to_value(&native).unwrap(), *expected);
    let from_ts: CodexUsage = js_json::from_value(expected).unwrap();
    let response: JsValue = ts.request(&native_json!({"id":92,"operation":"cases","cases":[{"name":"raw-format","kind":"format","usage":js_json::to_value(&native).unwrap()}]}));
    assert_eq!(response["ok"], native_json!(true));
    assert_eq!(
        native_json!(format_codex_usage(&from_ts)),
        response["output"]["results"][0]["text"]
    );
}
