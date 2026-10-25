use pi_js_compat::js_json::JsString;
use std::time::Duration;

use pi_ai::auth::oauth::anthropic::AnthropicOAuth;
use pi_ai::auth::oauth::pkce::generate_pkce_with;
use pi_ai::auth::oauth::support::RandomSource;
use pi_ai::auth::{AuthEvent, AuthPromptKind, OAuthAuth, OAuthCredential};

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};
use oauth_support::{
    PromptAnswer, TestInteraction, TestRandom, http_get, lock_callback_port, recorded_requests, runtime,
};
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const NOW: f64 = 1_750_000_000_000.0;

fn verifier() -> String {
    let random = TestRandom::new();
    generate_pkce_with(|bytes| random.fill(bytes)).unwrap().verifier
}

#[tokio::test]
async fn manual_code_login_posts_the_expected_exchange_body_and_cancels_the_prompt() {
    let _port = lock_callback_port().await;
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        r#"{"access_token":"access-token","refresh_token":"refresh-token","expires_in":3600}"#,
    )]);
    let oauth = AnthropicOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text(format!(
        "http://localhost:53692/callback?code=manual-code&state={}",
        verifier()
    ))]);

    let credential = oauth.login(interaction.clone()).await.expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), "access-token");
    assert_eq!(credential.refresh.to_utf8().unwrap(), "refresh-token");
    assert_eq!(credential.expires, NOW + 3_600_000.0 - 300_000.0);

    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].url, TOKEN_URL);
    assert_eq!(
        requests[0].headers.get("Content-Type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        requests[0].headers.get("Accept").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        requests[0].body,
        format!(
            r#"{{"grant_type":"authorization_code","client_id":"9d1c250a-e61b-44d9-88ed-5944d1962f5e","code":"manual-code","state":"{}","redirect_uri":"http://localhost:53692/callback","code_verifier":"{}"}}"#,
            verifier(),
            verifier()
        )
    );

    let events = interaction.events();
    assert!(matches!(&events[0], AuthEvent::AuthUrl { url, .. } if url.contains("code_challenge_method=S256")));
    assert!(
        matches!(&events[1], AuthEvent::Progress { message } if message == "Exchanging authorization code for tokens...")
    );
    let prompts = interaction.prompts();
    assert_eq!(prompts.len(), 1);
    assert!(matches!(prompts[0].kind, AuthPromptKind::ManualCode { .. }));
    assert!(prompts[0].signal.as_ref().expect("prompt signal").aborted());
}

#[tokio::test]
async fn callback_login_uses_the_local_server_and_redirect_uri() {
    let _port = lock_callback_port().await;
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        r#"{"access_token":"callback-access","refresh_token":"callback-refresh","expires_in":60}"#,
    )]);
    let oauth = AnthropicOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Pending]);
    let login = tokio::spawn({
        let interaction = interaction.clone();
        async move { oauth.login(interaction).await }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    let (status, body) = http_get(&format!(
        "http://127.0.0.1:53692/callback?code=server-code&state={}",
        verifier()
    ))
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Anthropic authentication completed. You can close this window."));

    let credential = login.await.expect("login task").expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), "callback-access");
    assert_eq!(credential.expires, NOW + 60_000.0 - 300_000.0);
    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 1);
    assert!(requests[0].body.contains(r#""code":"server-code""#));
    assert!(requests[0].body.contains(r#""state":""#));
}

#[tokio::test]
async fn refresh_omits_scope_and_applies_the_five_minute_skew() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600,"scope":"ignored"}"#,
    )]);
    let oauth = AnthropicOAuth::with_runtime(runtime(fetch.clone(), NOW));

    let refreshed = oauth
        .refresh(OAuthCredential::new("old-refresh", "old-access", 0.0))
        .await
        .expect("refreshed");
    assert_eq!(refreshed.access.to_utf8().unwrap(), "new-access");
    assert_eq!(refreshed.refresh.to_utf8().unwrap(), "new-refresh");
    assert_eq!(refreshed.expires, NOW + 3_600_000.0 - 300_000.0);
    assert!(
        !refreshed
            .extra
            .contains_key(&pi_js_compat::js_json::JsString::from("scope"))
    );
    assert_eq!(
        recorded_requests(&fetch)[0].body,
        r#"{"grant_type":"refresh_token","client_id":"9d1c250a-e61b-44d9-88ed-5944d1962f5e","refresh_token":"old-refresh"}"#
    );
}

#[tokio::test]
async fn to_auth_returns_the_access_token() {
    let oauth = AnthropicOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), NOW));
    assert_eq!(oauth.name(), "Anthropic (Claude Pro/Max)");
    let auth = oauth
        .to_auth(OAuthCredential::new("r", "token", 0.0))
        .await
        .expect("auth");
    assert_eq!(auth.api_key.as_ref(), Some(&JsString::from("token")));
    assert!(auth.headers.is_none());
}

#[tokio::test]
async fn manual_state_mismatch_fails_with_the_source_message() {
    let _port = lock_callback_port().await;
    let oauth = AnthropicOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text(
        "http://localhost:53692/callback?code=manual-code&state=wrong".to_owned(),
    )]);
    let error = oauth.login(interaction).await.expect_err("state mismatch");
    assert_eq!(error.detail(), Some(&"OAuth state mismatch".into()));
}

#[tokio::test]
async fn token_exchange_http_failures_keep_the_wrapped_details() {
    let _port = lock_callback_port().await;
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(500, "boom")]);
    let oauth = AnthropicOAuth::with_runtime(runtime(fetch, NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text(format!(
        "http://localhost:53692/callback?code=c&state={}",
        verifier()
    ))]);
    let error = oauth.login(interaction).await.expect_err("http failure");
    let detail = error.detail().expect("detail");
    assert!(detail.starts_with("Token exchange request failed. url=https://platform.claude.com/v1/oauth/token; redirect_uri=http://localhost:53692/callback; response_type=authorization_code; details=Error: HTTP request failed. status=500;"));
    assert!(detail.contains("body=boom"));
}
