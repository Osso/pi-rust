use pi_js_compat::js_json::JsString;
use std::time::Duration;

use pi_ai::auth::oauth::openrouter::OpenRouterOAuth;
use pi_ai::auth::{AuthEvent, OAuthAuth, OAuthCredential};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};
use oauth_support::{PromptAnswer, TestInteraction, TestRandom, http_get, recorded_requests, runtime};
const NOW: f64 = 1_750_000_000_000.0;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

fn verifier() -> String {
    let random = TestRandom::new();
    pi_ai::auth::oauth::pkce::generate_pkce_with(|bytes| {
        pi_ai::auth::oauth::support::RandomSource::fill(random.as_ref(), bytes)
    })
    .verifier
}

async fn wait_for_auth_url(interaction: &TestInteraction) -> JsString {
    for _ in 0..200 {
        if let Some(AuthEvent::AuthUrl { url, .. }) = interaction
            .events()
            .into_iter()
            .find(|event| matches!(event, AuthEvent::AuthUrl { .. }))
        {
            return url;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("auth_url event was not emitted");
}

#[tokio::test]
async fn callback_login_exchanges_the_code_for_a_permanent_key() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, r#"{"key":"sk-or-test"}"#)]);
    let oauth = OpenRouterOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Pending]);
    let login = tokio::spawn({
        let interaction = interaction.clone();
        async move { oauth.login(interaction).await }
    });

    let auth_url = wait_for_auth_url(&interaction).await;
    let parsed = url::Url::parse(&auth_url).expect("authorize URL");
    assert_eq!(parsed.origin().ascii_serialization(), "https://openrouter.ai");
    assert_eq!(parsed.path(), "/auth");
    assert_eq!(
        parsed
            .query_pairs()
            .find(|(name, _)| name == "code_challenge_method")
            .unwrap()
            .1,
        "S256"
    );
    let callback_url = parsed
        .query_pairs()
        .find(|(name, _)| name == "callback_url")
        .map(|(_, value)| value.into_owned())
        .expect("callback_url");
    assert!(callback_url.starts_with("http://127.0.0.1:"));
    assert!(callback_url.contains("/oauth/callback/00000000-0000-4000-8000-000000000000"));

    let (status, body) = http_get(&format!("{callback_url}?code=authorization-code")).await;
    assert_eq!(status, 200);
    assert!(body.contains("Signed in to OpenRouter. You may now close this page."));

    let credential = login.await.expect("task").expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), "sk-or-test");
    assert_eq!(credential.refresh.to_utf8().unwrap(), "");
    assert_eq!(credential.expires, MAX_SAFE_INTEGER);
    assert!(interaction.prompts()[0].signal.as_ref().unwrap().is_cancelled());

    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].url, "https://openrouter.ai/api/v1/auth/keys");
    assert_eq!(
        requests[0].headers.get("accept").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        requests[0].headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        requests[0].body,
        json!({
            "code": "authorization-code",
            "code_verifier": verifier(),
            "code_challenge_method": "S256",
        })
        .to_string()
    );
}

#[tokio::test]
async fn callback_exchange_failures_reach_the_page_and_the_login() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        403,
        r#"{"error":{"message":"invalid code"}}"#,
    )]);
    let oauth = OpenRouterOAuth::with_runtime(runtime(fetch, NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Pending]);
    let login = tokio::spawn({
        let interaction = interaction.clone();
        async move { oauth.login(interaction).await }
    });

    let auth_url = wait_for_auth_url(&interaction).await;
    let callback_url = url::Url::parse(&auth_url)
        .unwrap()
        .query_pairs()
        .find(|(name, _)| name == "callback_url")
        .map(|(_, value)| value.into_owned())
        .unwrap();
    let (status, body) = http_get(&format!("{callback_url}?code=bad-code")).await;
    assert_eq!(status, 502);
    assert!(body.contains("OpenRouter key exchange failed."));

    let error = login.await.expect("task").expect_err("exchange failure");
    assert_eq!(
        error.detail(),
        Some(&JsString::from(
            "OpenRouter OAuth key exchange failed (HTTP 403): invalid code"
        ))
    );
}

#[tokio::test]
async fn manual_codes_are_trimmed_and_exchanged_without_a_callback() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, r#"{"key":"sk-or-manual"}"#)]);
    let oauth = OpenRouterOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("  manual-code  ".to_owned())]);

    let credential = oauth.login(interaction).await.expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), "sk-or-manual");
    assert_eq!(
        recorded_requests(&fetch)[0].body,
        json!({"code": "manual-code", "code_verifier": verifier(), "code_challenge_method": "S256"}).to_string()
    );
}

#[tokio::test]
async fn empty_manual_input_fails_without_a_token_request() {
    let fetch = RecordingFetch::new(vec![]);
    let oauth = OpenRouterOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("   ".to_owned())]);

    let error = oauth.login(interaction).await.expect_err("missing code");
    assert_eq!(error.detail(), Some(&JsString::from("Missing authorization code")));
    assert!(recorded_requests(&fetch).is_empty());
}

#[tokio::test]
async fn refresh_is_identity_and_to_auth_uses_the_key() {
    let oauth = OpenRouterOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), NOW));
    assert_eq!(oauth.name(), "OpenRouter OAuth");
    assert_eq!(oauth.login_label(), Some("Sign in with OpenRouter"));
    assert!(!oauth.is_subscription());
    let credential = OAuthCredential::new("", "token", MAX_SAFE_INTEGER);
    let refreshed = oauth
        .refresh(credential.clone(), CancellationToken::new())
        .await
        .expect("refresh");
    assert_eq!(refreshed, credential);
    let auth = oauth.to_auth(credential).await.expect("auth");
    assert_eq!(auth.api_key.as_ref(), Some(&JsString::from("token")));
}
