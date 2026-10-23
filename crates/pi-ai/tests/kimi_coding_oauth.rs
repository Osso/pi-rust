use pi_ai::auth::oauth::kimi_coding::KimiCodingOAuth;
use pi_ai::auth::{AuthEvent, OAuthAuth, OAuthCredential};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::JsString;

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};
use oauth_support::{PromptAnswer, TestInteraction, recorded_requests, runtime};
const NOW: f64 = 1_750_000_000_000.0;
const HOST: &str = "https://auth.kimi.com";
const CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";

fn device_body() -> String {
    serde_json::json!({
        "user_code": "ABCD-1234",
        "device_code": "device-code-123",
        "verification_uri": "https://www.kimi.com/code",
        "verification_uri_complete": "https://www.kimi.com/code?user_code=ABCD-1234",
        "interval": 5,
        "expires_in": 600
    })
    .to_string()
}

fn token_body(access: &str, refresh: &str) -> String {
    serde_json::json!({
        "access_token": access,
        "refresh_token": refresh,
        "expires_in": 3600
    })
    .to_string()
}

#[tokio::test(start_paused = true)]
async fn device_login_waits_before_the_first_poll_and_uses_the_complete_uri() {
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(200, &device_body()),
        ScriptedResponse::json(400, r#"{"error":"authorization_pending"}"#),
        ScriptedResponse::json(200, &token_body("access-token", "refresh-token")),
    ]);
    let oauth = KimiCodingOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Error("unexpected prompt".to_owned())]);

    let credential = oauth.login(interaction.clone()).await.expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), "access-token");
    assert_eq!(credential.refresh.to_utf8().unwrap(), "refresh-token");
    assert_eq!(credential.expires, NOW + 3_600_000.0);
    assert_eq!(
        interaction.events()[0],
        AuthEvent::DeviceCode {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://www.kimi.com/code?user_code=ABCD-1234".into(),
            interval_seconds: Some(5.0),
            expires_in_seconds: Some(600.0),
        }
    );

    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].url, format!("{HOST}/api/oauth/device_authorization"));
    assert_eq!(requests[0].body, format!("client_id={CLIENT_ID}"));
    assert_eq!(requests[1].url, format!("{HOST}/api/oauth/token"));
    assert_eq!(
        requests[1].body,
        format!(
            "client_id={CLIENT_ID}&device_code=device-code-123&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"
        )
    );
}

#[tokio::test(start_paused = true)]
async fn device_errors_have_source_messages() {
    for (error, message) in [
        (
            "expired_token",
            "Kimi Code device authorization expired. Please restart login.",
        ),
        ("access_denied", "Kimi Code login was denied."),
    ] {
        let fetch = RecordingFetch::new(vec![
            ScriptedResponse::json(200, &device_body()),
            ScriptedResponse::json(400, &serde_json::json!({"error": error}).to_string()),
        ]);
        let oauth = KimiCodingOAuth::with_runtime(runtime(fetch, NOW));
        let interaction = TestInteraction::new(vec![PromptAnswer::Error("unexpected prompt".to_owned())]);
        let error = oauth.login(interaction).await.expect_err("device error");
        assert_eq!(error.detail(), Some(&JsString::from(message)));
    }
}

#[tokio::test(start_paused = true)]
async fn honors_the_oauth_host_override_and_strips_trailing_slashes() {
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(200, &device_body()),
        ScriptedResponse::json(200, &token_body("a", "r")),
    ]);
    let oauth =
        KimiCodingOAuth::with_runtime(runtime(fetch.clone(), NOW)).with_oauth_host("https://auth.example.com///");
    let interaction = TestInteraction::new(vec![PromptAnswer::Error("unexpected prompt".to_owned())]);
    oauth.login(interaction).await.expect("credential");
    let urls: Vec<String> = recorded_requests(&fetch)
        .into_iter()
        .map(|request| request.url)
        .collect();
    assert_eq!(
        urls,
        vec![
            "https://auth.example.com/api/oauth/device_authorization".to_owned(),
            "https://auth.example.com/api/oauth/token".to_owned()
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn refresh_retries_429_then_succeeds_and_rejects_invalid_grant() {
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(429, r#"{"error":"temporarily_unavailable"}"#),
        ScriptedResponse::json(200, &token_body("a", "r")),
    ]);
    let oauth = KimiCodingOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let refreshed = oauth
        .refresh(OAuthCredential::new("old", "old", 0.0), AbortSignal::default())
        .await
        .expect("refreshed");
    assert_eq!(refreshed.access.to_utf8().unwrap(), "a");
    assert_eq!(refreshed.refresh.to_utf8().unwrap(), "r");
    assert_eq!(recorded_requests(&fetch).len(), 2);

    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        400,
        r#"{"error":"invalid_grant","error_description":"expired"}"#,
    )]);
    let oauth = KimiCodingOAuth::with_runtime(runtime(fetch, NOW));
    let error = oauth
        .refresh(OAuthCredential::new("old", "old", 0.0), AbortSignal::default())
        .await
        .expect_err("invalid grant");
    assert_eq!(
        error.detail(),
        Some(&JsString::from(
            "Kimi Code token refresh unauthorized (status 400): expired"
        ))
    );
}

#[tokio::test]
async fn to_auth_returns_a_bearer_header_and_labels_match() {
    let oauth = KimiCodingOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), NOW));
    assert_eq!(oauth.name(), "Kimi Code (subscription)");
    assert_eq!(oauth.login_label(), Some("Sign in with Kimi Code"));
    assert!(oauth.is_subscription());
    let auth = oauth
        .to_auth(OAuthCredential::new("r", "access", 0.0))
        .await
        .expect("auth");
    assert_eq!(auth.api_key, None);
    assert_eq!(
        auth.headers.unwrap().get("Authorization").cloned().flatten(),
        Some(JsString::from("Bearer access"))
    );
}
