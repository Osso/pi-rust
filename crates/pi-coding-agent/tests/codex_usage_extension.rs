//! Behavioral tests for the bundled `codex-usage` extension.

#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/extensions.rs"]
mod support;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::Arc;

use bundled::{Harness, HarnessOptions, StubFetch, test_model};
use pi_coding_agent::core::extensions::types::{CustomMessageContent, ExtensionApi, NotifyType};
use pi_coding_agent::core::extensions::{Extension, ExtensionRuntime, create_extension_runtime};
use pi_js_compat::base64::buffer_to_base64;
use serde_json::{Value, json};

fn base64url(bytes: &[u8]) -> String {
    buffer_to_base64(bytes)
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_owned()
}

fn token_with_claims(claims: Value) -> String {
    format!(
        "header.{}.signature",
        base64url(serde_json::to_string(&claims).unwrap().as_bytes())
    )
}

fn usage_payload(overrides: Value) -> Value {
    let mut payload = json!({
        "plan_type": "pro",
        "rate_limit": {
            "primary_window": {"used_percent": 42, "limit_window_seconds": 18_000, "reset_at": 1_800_000_000},
            "secondary_window": {"used_percent": 84, "limit_window_seconds": 604_800, "reset_at": 1_800_604_800}
        },
        "credits": {"has_credits": true, "unlimited": false, "balance": "9.99"},
        "rate_limit_reset_credits": {"available_count": 3}
    });
    if let (Some(payload), Some(overrides)) = (payload.as_object_mut(), overrides.as_object()) {
        for (key, value) in overrides {
            payload.insert(key.clone(), value.clone());
        }
    }
    payload
}

async fn load(runtime: &ExtensionRuntime, fetch: Arc<StubFetch>) -> Extension {
    let options = pi_coding_agent::bundled_extensions::codex_usage::CodexUsageOptions {
        fetch: Some(fetch as Arc<dyn pi_ai::request::Fetch>),
    };
    support::extension(
        runtime,
        "/repo",
        "<first-party:codex-usage>",
        move |pi: &ExtensionApi| {
            pi_coding_agent::bundled_extensions::codex_usage::codex_usage_extension(pi, &options)
                .expect("codex-usage extension")
        },
    )
    .await
}

fn auth(token: &str) -> Option<pi_js_compat::js_json::JsObject> {
    JsValue::from(json!({"openai-codex": {
        "type": "oauth",
        "refresh": "refresh-token",
        "access": token,
        "expires": 9_999_999_999_999.0
    }}))
    .as_object()
    .cloned()
}

#[test]
fn maps_usage_payload() {
    use pi_coding_agent::bundled_extensions::codex_usage::map_codex_usage_payload;

    let usage = map_codex_usage_payload(&usage_payload(json!({})));
    assert_eq!(usage.plan_type.as_deref(), Some("pro"));
    assert_eq!(usage.account_id, None);
    assert_eq!(usage.reset_credits_available, Some(3.0));
    assert_eq!(usage.limits.len(), 1);
    let limit = &usage.limits[0];
    assert_eq!(limit.limit_id, "codex");
    let primary = limit.primary.as_ref().expect("primary");
    assert_eq!(primary.used_percent, 42.0);
    assert_eq!(primary.window_minutes, Some(300.0));
    assert_eq!(primary.resets_at, Some(1_800_000_000.0));
    let secondary = limit.secondary.as_ref().expect("secondary");
    assert_eq!(secondary.window_minutes, Some(10_080.0));
    let credits = limit.credits.as_ref().expect("credits");
    assert!(credits.has_credits);
    assert_eq!(credits.balance.as_deref(), Some("9.99"));
}

#[test]
fn maps_additional_rate_limits() {
    use pi_coding_agent::bundled_extensions::codex_usage::map_codex_usage_payload;

    let usage = map_codex_usage_payload(&usage_payload(json!({
        "additional_rate_limits": [{
            "limit_name": "codex_weekly",
            "metered_feature": "codex_weekly",
            "rate_limit": {"primary_window": {"used_percent": 70, "limit_window_seconds": 604_800, "reset_at": 1_800_604_800}}
        }]
    })));
    let ids: Vec<&str> = usage.limits.iter().map(|limit| limit.limit_id.as_str()).collect();
    assert_eq!(ids, ["codex", "codex_weekly"]);
    assert_eq!(usage.limits[1].primary.as_ref().unwrap().used_percent, 70.0);
}

#[test]
fn formats_usage_text() {
    use pi_coding_agent::bundled_extensions::codex_usage::{format_codex_usage, map_codex_usage_payload};

    let mut usage = map_codex_usage_payload(&usage_payload(json!({})));
    usage.account_id = Some("acct_123".to_owned().into());
    usage.email = Some("user@example.com".to_owned().into());
    let text = format_codex_usage(&usage);
    assert_eq!(
        text,
        "OpenAI Codex usage\nUser: user@example.com\nAccount: acct_123\nPlan: pro\nReset credits: 3 available\ncodex: 5-hour usage 42% of 300m window resets 2027-01-15 08:00\ncodex: weekly usage 84% of 10080m window resets 2027-01-22 08:00\ncodex: credits balance 9.99"
    );
}

#[test]
fn extracts_account_and_email_claims() {
    use pi_coding_agent::bundled_extensions::codex_usage::{
        resolve_codex_account_id, resolve_codex_account_info, resolve_codex_display_email,
    };

    let token = token_with_claims(json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"},
        "email": "user@example.com"
    }));
    assert_eq!(resolve_codex_account_id(&JsString::from(&token)).unwrap(), "acct_123");
    let info = resolve_codex_account_info(&JsString::from(&token)).unwrap();
    assert_eq!(info.account_id, "acct_123");
    assert_eq!(info.email.as_ref(), Some(&JsString::from("user@example.com")));

    let profile_token = token_with_claims(json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"},
        "profile": {"email": "profile@example.com"}
    }));
    assert_eq!(
        resolve_codex_display_email(&JsString::from(&profile_token), None)
            .unwrap()
            .as_ref(),
        Some(&JsString::from("profile@example.com"))
    );
    assert_eq!(
        resolve_codex_display_email(
            &JsString::from(&profile_token),
            Some(&JsValue::from(&json!("stored@example.com")))
        )
        .unwrap()
        .as_ref(),
        Some(&JsString::from("stored@example.com"))
    );

    assert_eq!(
        resolve_codex_account_info(&JsString::from("not-a-token")).unwrap_err(),
        "Invalid OpenAI Codex access token"
    );
    let missing = token_with_claims(json!({}));
    assert_eq!(
        resolve_codex_account_info(&JsString::from(&missing)).unwrap_err(),
        "OpenAI Codex access token does not include a ChatGPT account id"
    );
}

#[tokio::test]
async fn usage_command_sends_formatted_message_and_headers() {
    let token = token_with_claims(json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"},
        "email": "user@example.com"
    }));
    let fetch = Arc::new(StubFetch::new(vec![(200, "", &usage_payload(json!({})).to_string())]));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, Arc::clone(&fetch)).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(false));
    options.auth = auth(&token);
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let handler = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .handler
        .clone();

    handler(String::new().into(), harness.command_context())
        .await
        .expect("usage");

    let messages = harness.actions.messages.lock().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].custom_type, "codex-usage");
    assert!(messages[0].display);
    let CustomMessageContent::Text(text) = &messages[0].content else {
        panic!("text message");
    };
    assert!(text.contains("User: user@example.com"));
    assert!(text.contains("Account: acct_123"));

    let requests = fetch.requests.lock().unwrap();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].url, "https://chatgpt.com/backend-api/wham/usage");
    let header = |name: &str| {
        requests[0]
            .headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    assert_eq!(header("authorization"), Some(format!("Bearer {token}")));
    assert_eq!(header("chatgpt-account-id"), Some("acct_123".to_owned()));
    assert_eq!(header("originator"), Some("pi".to_owned()));
    assert_eq!(header("user-agent"), Some("pi".to_owned()));
}

#[tokio::test]
async fn usage_command_argument_completions() {
    let fetch = Arc::new(StubFetch::new(Vec::new()));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, fetch).await;
    let completions = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .get_argument_completions
        .clone()
        .expect("completions");

    assert_eq!(
        completions(&"re".into()).unwrap(),
        Some(vec![pi_js_compat::js_json!({"value": "reset", "label": "reset"})])
    );
    assert_eq!(
        completions(&"a".into()).unwrap(),
        Some(vec![pi_js_compat::js_json!({"value": "all", "label": "all"})])
    );
    assert_eq!(completions(&"x".into()).unwrap(), None);
}

#[tokio::test]
async fn usage_command_rejects_extra_arguments() {
    let fetch = Arc::new(StubFetch::new(Vec::new()));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, fetch).await;
    let harness = Harness::new(vec![extension.clone()], runtime, HarnessOptions::new("/repo")).await;
    let handler = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .handler
        .clone();

    handler("bogus".to_owned().into(), harness.command_context())
        .await
        .expect("usage");

    assert_eq!(
        harness.ui.last_notification(),
        Some((JsString::from("Usage: /usage [all|reset]"), Some(NotifyType::Warning)))
    );
}

#[tokio::test]
async fn reset_asks_for_confirmation_before_consuming() {
    let token = token_with_claims(json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"}
    }));
    let fetch = Arc::new(StubFetch::new(vec![(200, "", &usage_payload(json!({})).to_string())]));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, Arc::clone(&fetch)).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(false));
    options.auth = auth(&token);
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    harness.ui.confirm_result(false);
    let handler = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .handler
        .clone();

    handler("reset".to_owned().into(), harness.command_context())
        .await
        .expect("reset");

    assert_eq!(fetch.requests.lock().unwrap().len(), 1);
    assert!(harness.actions.messages.lock().unwrap().is_empty());
}

#[tokio::test]
async fn reset_consumes_credit_and_reports_new_usage() {
    let token = token_with_claims(json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"}
    }));
    let fetch = Arc::new(StubFetch::new(vec![
        (200, "", &usage_payload(json!({})).to_string()),
        (200, "", &json!({"code": "reset", "windows_reset": 2}).to_string()),
        (
            200,
            "",
            &usage_payload(json!({"rate_limit_reset_credits": {"available_count": 2}})).to_string(),
        ),
    ]));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, Arc::clone(&fetch)).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(false));
    options.auth = auth(&token);
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    harness.ui.confirm_result(true);
    let handler = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .handler
        .clone();

    handler("reset".to_owned().into(), harness.command_context())
        .await
        .expect("reset");

    let requests = fetch.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[1].method, "POST");
    assert_eq!(
        requests[1].url,
        "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume"
    );
    let body: Value = serde_json::from_slice(requests[1].body.as_ref().expect("body")).expect("json body");
    assert!(body["redeem_request_id"].as_str().is_some_and(|id| !id.is_empty()));

    let messages = harness.actions.messages.lock().unwrap();
    let CustomMessageContent::Text(text) = &messages[0].content else {
        panic!("text message");
    };
    assert!(text.starts_with("Consumed reset credit. Windows reset: 2\n\n"));
    assert!(text.contains("Reset credits: 2 available"));
}

#[tokio::test]
async fn reset_without_credits_reports_unavailable() {
    let token = token_with_claims(json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"}
    }));
    let fetch = Arc::new(StubFetch::new(vec![(
        200,
        "",
        &usage_payload(json!({"rate_limit_reset_credits": {"available_count": 0}})).to_string(),
    )]));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, Arc::clone(&fetch)).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(false));
    options.auth = auth(&token);
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let handler = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .handler
        .clone();

    handler("reset".to_owned().into(), harness.command_context())
        .await
        .expect("reset");

    assert_eq!(fetch.requests.lock().unwrap().len(), 1);
    let messages = harness.actions.messages.lock().unwrap();
    let CustomMessageContent::Text(text) = &messages[0].content else {
        panic!("text message");
    };
    assert!(text.ends_with("No reset credits available."));
}

#[tokio::test]
async fn reports_missing_login() {
    let fetch = Arc::new(StubFetch::new(Vec::new()));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, fetch).await;
    let harness = Harness::new(vec![extension.clone()], runtime, HarnessOptions::new("/repo")).await;
    let handler = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .handler
        .clone();

    let error = handler(String::new().into(), harness.command_context())
        .await
        .unwrap_err();

    assert_eq!(
        error.message,
        "OpenAI Codex provider openai-codex is not logged in. Run /login openai-codex."
    );
}

#[tokio::test]
async fn reports_failed_usage_requests() {
    let token = token_with_claims(json!({
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct_123"}
    }));
    let fetch = Arc::new(StubFetch::new(vec![(500, "Server Error", "boom")]));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, Arc::clone(&fetch)).await;
    let mut options = HarnessOptions::new("/repo");
    options.model = Some(test_model(false));
    options.auth = auth(&token);
    let harness = Harness::new(vec![extension.clone()], runtime, options).await;
    let handler = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("usage"))
        .expect("usage command")
        .handler
        .clone();

    let error = handler(String::new().into(), harness.command_context())
        .await
        .unwrap_err();

    assert_eq!(error.message, "OpenAI Codex usage request failed (500): boom");
}

#[test]
fn formats_reset_responses() {
    use pi_coding_agent::bundled_extensions::codex_usage::{ConsumeResetCreditResponse, format_reset_response};

    let cases = [
        ("reset", Some(2.0), "Consumed reset credit. Windows reset: 2"),
        (
            "nothing_to_reset",
            None,
            "No active Codex rate-limit window needed a reset.",
        ),
        ("no_credit", None, "No reset credit available."),
        ("already_redeemed", None, "Reset credit was already redeemed."),
        ("other", None, "Reset credit response received."),
    ];
    for (code, windows_reset, expected) in cases {
        let response = ConsumeResetCreditResponse {
            code: Some(code.to_owned()),
            windows_reset,
        };
        assert_eq!(format_reset_response(&response), expected);
    }
}

#[tokio::test]
async fn usage_http_payload_and_report_keep_raw_human_text() {
    let token = token_with_claims(json!({"https://api.openai.com/auth":{"chatgpt_account_id":"acct"}}));
    let body = r#"{"plan_type":"pro\ud800", "credits":{"balance":"10\udfff"}, "additional_rate_limits":[{"limit_name":"rate\ud801"}]}"#;
    let fetch = Arc::new(StubFetch::new(vec![(200, "", body)]));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, fetch).await;
    let mut options = HarnessOptions::new("/repo");
    options.auth = auth(&token);
    options.model = Some(test_model(false));
    let handler = extension.read().commands["usage"].handler.clone();
    let harness = Harness::new(vec![extension], runtime, options).await;
    handler("".into(), harness.command_context()).await.unwrap();
    let messages = harness.actions.messages.lock().unwrap();
    let CustomMessageContent::Text(text) = &messages[0].content else {
        panic!("text");
    };
    assert!(text.contains(JsString::from("Plan: pro") + JsString::from_utf16(vec![0xd800])));
    assert!(text.contains(JsString::from("credits balance 10") + JsString::from_utf16(vec![0xdfff])));
    assert!(text.contains(JsString::from("rate") + JsString::from_utf16(vec![0xd801])));
}

#[tokio::test]
async fn busy_usage_read_notifies_and_queues_next_turn() {
    let token = token_with_claims(json!({"https://api.openai.com/auth":{"chatgpt_account_id":"acct"}}));
    let fetch = Arc::new(StubFetch::new(vec![(200, "", "{}")]));
    let runtime = create_extension_runtime();
    let extension = load(&runtime, fetch).await;
    let mut options = HarnessOptions::new("/repo");
    options.auth = auth(&token);
    let handler = extension.read().commands["usage"].handler.clone();
    let harness = Harness::new(vec![extension], runtime, options).await;
    harness.idle.store(false, std::sync::atomic::Ordering::SeqCst);
    handler("".into(), harness.command_context()).await.unwrap();
    assert_eq!(harness.ui.notifications().len(), 1);
    assert_eq!(
        harness.actions.message_options.lock().unwrap()[0]
            .as_ref()
            .unwrap()
            .deliver_as,
        Some(pi_coding_agent::core::extensions::types::MessageDelivery::NextTurn)
    );
}
