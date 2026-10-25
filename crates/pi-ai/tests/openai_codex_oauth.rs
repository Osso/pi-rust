use pi_js_compat::js_json::JsString;
use std::time::Duration;

use pi_ai::auth::oauth::openai_codex::OpenAICodexOAuth;
use pi_ai::auth::{AuthEvent, AuthPromptKind, OAuthAuth, OAuthCredential};
use serde_json::json;

use crate::shared_oauth as oauth_support;

use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};
use oauth_support::{
    PromptAnswer, TestInteraction, TestRandom, http_get, lock_callback_port, recorded_requests, runtime,
};
const NOW: f64 = 1_750_000_000_000.0;
const USER_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let value = (a << 16) | (b << 8) | c;
        out.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn access_token(account_id: &str) -> String {
    let header = base64(br#"{"alg":"none"}"#);
    let payload = base64(
        json!({"https://api.openai.com/auth": {"chatgpt_account_id": account_id}})
            .to_string()
            .as_bytes(),
    );
    format!("{header}.{payload}.signature")
}

fn state() -> String {
    let random = TestRandom::new();
    let mut bytes = [0_u8; 16];
    pi_ai::auth::oauth::support::RandomSource::fill(random.as_ref(), &mut bytes).unwrap();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[tokio::test(start_paused = true)]
async fn device_code_login_notifies_polls_and_extracts_the_account_id() {
    let token = access_token("account-123");
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(
            200,
            r#"{"device_auth_id":"device-auth-id","user_code":"ABCD-1234","interval":"5"}"#,
        ),
        ScriptedResponse::json(
            403,
            r#"{"error":{"message":"pending","code":"deviceauth_authorization_pending"}}"#,
        ),
        ScriptedResponse::json(
            200,
            r#"{"authorization_code":"oauth-code","code_challenge":"c","code_verifier":"device-code-verifier"}"#,
        ),
        ScriptedResponse::json(
            200,
            &json!({"access_token": token, "refresh_token": "refresh-token", "expires_in": 3600}).to_string(),
        ),
    ]);
    let oauth = OpenAICodexOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("device_code".to_owned())]);

    let credential = oauth.login(interaction.clone()).await.expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), token);
    assert_eq!(credential.refresh.to_utf8().unwrap(), "refresh-token");
    assert_eq!(credential.expires, NOW + 3_600_000.0);
    assert_eq!(
        credential
            .extra
            .get(&pi_js_compat::js_json::JsString::from("accountId")),
        Some(&pi_js_compat::js_json::JsValue::from(&json!("account-123")))
    );

    let events = interaction.events();
    assert_eq!(
        events[0],
        AuthEvent::DeviceCode {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://auth.openai.com/codex/device".into(),
            interval_seconds: Some(5.0),
            expires_in_seconds: Some(900.0),
        }
    );
    let prompts = interaction.prompts();
    assert!(
        matches!(&prompts[0].kind, AuthPromptKind::Select { message, options } if message == "Select OpenAI Codex login method:" && options.len() == 2)
    );

    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].url, USER_CODE_URL);
    assert_eq!(
        requests[0].headers.get("Content-Type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(requests[0].body, r#"{"client_id":"app_EMoamEEZ73f0CkXaXp7hrann"}"#);
    assert_eq!(requests[1].url, DEVICE_TOKEN_URL);
    assert_eq!(
        requests[1].body,
        r#"{"device_auth_id":"device-auth-id","user_code":"ABCD-1234"}"#
    );
    assert_eq!(requests[3].url, TOKEN_URL);
    assert_eq!(
        requests[3].body,
        "grant_type=authorization_code&client_id=app_EMoamEEZ73f0CkXaXp7hrann&code=oauth-code&code_verifier=device-code-verifier&redirect_uri=https%3A%2F%2Fauth.openai.com%2Fdeviceauth%2Fcallback"
    );
}

#[tokio::test]
async fn browser_login_hits_the_local_callback_and_uses_the_browser_redirect_uri() {
    let _port = lock_callback_port().await;
    let token = access_token("account-browser");
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        &json!({"access_token": token, "refresh_token": "refresh-token", "expires_in": 60}).to_string(),
    )]);
    let oauth = OpenAICodexOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("browser".to_owned()), PromptAnswer::Pending]);
    let login = tokio::spawn({
        let interaction = interaction.clone();
        async move { oauth.login(interaction).await }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    let (status, body) = http_get(&format!(
        "http://127.0.0.1:1455/auth/callback?code=browser-code&state={}",
        state()
    ))
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("OpenAI authentication completed. You can close this window."));

    let credential = login.await.expect("task").expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), token);
    assert_eq!(
        credential
            .extra
            .get(&pi_js_compat::js_json::JsString::from("accountId")),
        Some(&pi_js_compat::js_json::JsValue::from(&json!("account-browser")))
    );

    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].body,
        "grant_type=authorization_code&client_id=app_EMoamEEZ73f0CkXaXp7hrann&code=browser-code&code_verifier=AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA&redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"
    );
    assert!(matches!(&interaction.events()[0], AuthEvent::AuthUrl { url, .. } if url.contains("originator=pi")));
}

#[tokio::test]
async fn refresh_posts_the_rotated_token_and_reads_the_account_id() {
    let token = access_token("account-refresh");
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        &json!({"access_token": token, "refresh_token": "new-refresh", "expires_in": 3600}).to_string(),
    )]);
    let oauth = OpenAICodexOAuth::with_runtime(runtime(fetch.clone(), NOW));

    let refreshed = oauth
        .refresh(OAuthCredential::new("old-refresh", "old-access", 0.0))
        .await
        .expect("refreshed");
    assert_eq!(refreshed.refresh.to_utf8().unwrap(), "new-refresh");
    assert_eq!(refreshed.expires, NOW + 3_600_000.0);
    assert_eq!(
        refreshed.extra.get(&pi_js_compat::js_json::JsString::from("accountId")),
        Some(&pi_js_compat::js_json::JsValue::from(&json!("account-refresh")))
    );
    assert_eq!(
        recorded_requests(&fetch)[0].body,
        "grant_type=refresh_token&refresh_token=old-refresh&client_id=app_EMoamEEZ73f0CkXaXp7hrann"
    );
}

#[tokio::test]
async fn base64url_jwt_payloads_fail_like_atob() {
    let token = (0xa0..0x3000_u32)
        .filter_map(char::from_u32)
        .map(|character| access_token(&format!("account{character}")))
        .find(|token| token.contains('+'))
        .expect("some account id encodes to a standard-base64 plus")
        .replace('+', "-");
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        &json!({"access_token": token, "refresh_token": "new-refresh", "expires_in": 3600}).to_string(),
    )]);
    let oauth = OpenAICodexOAuth::with_runtime(runtime(fetch, NOW));

    let error = oauth
        .refresh(OAuthCredential::new("old-refresh", "old-access", 0.0))
        .await
        .expect_err("atob rejects base64url '-'");
    assert_eq!(
        error.detail(),
        Some(&JsString::from("Failed to extract accountId from token"))
    );
}

#[tokio::test]
async fn rejects_unknown_login_methods_and_missing_account_ids() {
    let oauth = OpenAICodexOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("carrier-pigeon".to_owned())]);
    let error = oauth.login(interaction).await.expect_err("unknown method");
    assert_eq!(
        error.detail(),
        Some(&JsString::from("Unknown OpenAI Codex login method: carrier-pigeon"))
    );

    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(200, r#"{"device_auth_id":"id","user_code":"CODE","interval":"1"}"#),
        ScriptedResponse::json(200, r#"{"authorization_code":"code","code_verifier":"verifier"}"#),
        ScriptedResponse::json(
            200,
            r#"{"access_token":"not-a-jwt","refresh_token":"r","expires_in":3600}"#,
        ),
    ]);
    let oauth = OpenAICodexOAuth::with_runtime(runtime(fetch, NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("device_code".to_owned())]);
    let error = oauth.login(interaction).await.expect_err("account id");
    assert_eq!(
        error.detail(),
        Some(&JsString::from("Failed to extract accountId from token"))
    );
}

#[tokio::test]
async fn device_code_404_has_the_source_message() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(404, "not found")]);
    let oauth = OpenAICodexOAuth::with_runtime(runtime(fetch, NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("device_code".to_owned())]);
    let error = oauth.login(interaction).await.expect_err("404");
    assert_eq!(
        error.detail(),
        Some(&JsString::from(
            "OpenAI Codex device code login is not enabled for this server. Use browser login or verify the server URL."
        ))
    );
}

#[tokio::test]
async fn to_auth_returns_the_access_token() {
    let oauth = OpenAICodexOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), NOW));
    assert_eq!(oauth.name(), "OpenAI (ChatGPT Plus/Pro)");
    let auth = oauth
        .to_auth(OAuthCredential::new("r", "token", 0.0))
        .await
        .expect("auth");
    assert_eq!(auth.api_key.as_ref(), Some(&JsString::from("token")));
}

#[tokio::test(start_paused = true)]
async fn device_code_interval_uses_javascript_number_parsing() {
    // `Number("0x10".trim())` is 16.
    let token = access_token("account-hex");
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(
            200,
            r#"{"device_auth_id":"device-auth-id","user_code":"CODE","interval":"0x10"}"#,
        ),
        ScriptedResponse::json(
            200,
            r#"{"authorization_code":"oauth-code","code_challenge":"c","code_verifier":"device-code-verifier"}"#,
        ),
        ScriptedResponse::json(
            200,
            &json!({"access_token": token, "refresh_token": "refresh-token", "expires_in": 3600}).to_string(),
        ),
    ]);
    let oauth = OpenAICodexOAuth::with_runtime(runtime(fetch, NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("device_code".to_owned())]);

    oauth.login(interaction.clone()).await.expect("credential");

    let events = interaction.events();
    assert!(
        matches!(
            &events[0],
            AuthEvent::DeviceCode {
                interval_seconds: Some(interval),
                ..
            } if *interval == 16.0
        ),
        "unexpected events: {events:?}"
    );
}
