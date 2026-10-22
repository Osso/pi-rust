use pi_ai::auth::oauth::xai::XaiOAuth;
use pi_ai::auth::{AuthEvent, OAuthAuth, OAuthCredential};
use pi_js_compat::js_json::JsString;
use tokio_util::sync::CancellationToken;

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};
use oauth_support::{PromptAnswer, TestInteraction, recorded_requests, runtime};
const NOW: f64 = 1_750_000_000_000.0;
const DEVICE_URL: &str = "https://auth.x.ai/oauth2/device/code";
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";

fn device_response(overrides: serde_json::Value) -> ScriptedResponse {
    let mut body = serde_json::json!({
        "device_code": "device-code",
        "user_code": "ABCD-1234",
        "verification_uri": "https://accounts.x.ai/oauth2/device",
        "expires_in": 900,
        "interval": 5
    });
    for (key, value) in overrides.as_object().unwrap() {
        body[key] = value.clone();
    }
    ScriptedResponse::json(200, &body.to_string())
}

fn token_body(overrides: serde_json::Value) -> String {
    let mut body = serde_json::json!({
        "access_token": "access-token",
        "refresh_token": "refresh-token",
        "expires_in": 21600,
        "token_type": "Bearer"
    });
    for (key, value) in overrides.as_object().unwrap() {
        if value.is_null() {
            body.as_object_mut().unwrap().remove(key);
        } else {
            body[key] = value.clone();
        }
    }
    body.to_string()
}

#[tokio::test(start_paused = true)]
async fn device_flow_delays_polling_and_handles_pending_and_slow_down() {
    let fetch = RecordingFetch::new(vec![
        device_response(serde_json::json!({})),
        ScriptedResponse::json(400, r#"{"error":"authorization_pending"}"#),
        ScriptedResponse::json(400, r#"{"error":"slow_down","interval":10}"#),
        ScriptedResponse::json(200, &token_body(serde_json::json!({}))),
    ]);
    let oauth = XaiOAuth::with_runtime(runtime(fetch.clone(), NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Error("unexpected prompt".to_owned())]);

    let credential = oauth.login(interaction.clone()).await.expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), "access-token");
    assert_eq!(credential.refresh.to_utf8().unwrap(), "refresh-token");
    assert_eq!(credential.expires, NOW + 21_600_000.0 - 300_000.0);
    assert_eq!(
        interaction.events()[0],
        AuthEvent::DeviceCode {
            user_code: ("ABCD-1234".to_owned()).into(),
            verification_uri: ("https://accounts.x.ai/oauth2/device".to_owned()).into(),
            interval_seconds: Some(5.0),
            expires_in_seconds: Some(900.0),
        }
    );

    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].url, DEVICE_URL);
    assert_eq!(
        requests[0].headers.get("Accept").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        requests[0].headers.get("Content-Type").map(String::as_str),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        requests[0].body,
        "client_id=b1a00492-073a-47ea-816f-4c329264a828&scope=openid+profile+email+offline_access+grok-cli%3Aaccess+api%3Aaccess&referrer=pi"
    );
    assert_eq!(requests[3].url, TOKEN_URL);
    assert_eq!(
        requests[3].body,
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id=b1a00492-073a-47ea-816f-4c329264a828&device_code=device-code"
    );
}

#[tokio::test(start_paused = true)]
async fn prefers_verification_uri_complete_and_defaults_interval_zero() {
    let fetch = RecordingFetch::new(vec![
        device_response(serde_json::json!({
            "interval": 0,
            "verification_uri_complete": "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234"
        })),
        ScriptedResponse::json(200, &token_body(serde_json::json!({}))),
    ]);
    let oauth = XaiOAuth::with_runtime(runtime(fetch, NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Error("unexpected prompt".to_owned())]);
    oauth.login(interaction.clone()).await.expect("credential");
    assert_eq!(
        interaction.events()[0],
        AuthEvent::DeviceCode {
            user_code: ("ABCD-1234".to_owned()).into(),
            verification_uri: ("https://accounts.x.ai/oauth2/device?user_code=ABCD-1234".to_owned()).into(),
            interval_seconds: None,
            expires_in_seconds: Some(900.0),
        }
    );
}

#[tokio::test]
async fn rejects_untrusted_verification_uris() {
    for value in ["http://accounts.x.ai/oauth2/device", "file:///etc/passwd", "not a url"] {
        let fetch = RecordingFetch::new(vec![device_response(serde_json::json!({
            "verification_uri": value
        }))]);
        let oauth = XaiOAuth::with_runtime(runtime(fetch, NOW));
        let interaction = TestInteraction::new(vec![PromptAnswer::Error("unexpected prompt".to_owned())]);
        let error = oauth.login(interaction).await.expect_err("untrusted URI");
        assert_eq!(
            error.detail(),
            Some(&JsString::from("Untrusted verification URI in xAI OAuth response"))
        );
    }

    let fetch = RecordingFetch::new(vec![device_response(serde_json::json!({
        "verification_uri_complete": "http://accounts.x.ai/oauth2/device?user_code=ABCD-1234"
    }))]);
    let oauth = XaiOAuth::with_runtime(runtime(fetch, NOW));
    let interaction = TestInteraction::new(vec![PromptAnswer::Error("unexpected prompt".to_owned())]);
    let error = oauth.login(interaction).await.expect_err("untrusted URI");
    assert_eq!(
        error.detail(),
        Some(&JsString::from("Untrusted verification URI in xAI OAuth response"))
    );
}

#[tokio::test(start_paused = true)]
async fn denied_and_expired_device_codes_have_source_messages() {
    for (error_code, message) in [
        ("access_denied", "xAI device authorization was denied"),
        ("authorization_denied", "xAI device authorization was denied"),
        ("expired_token", "xAI device code expired"),
    ] {
        let fetch = RecordingFetch::new(vec![
            device_response(serde_json::json!({"interval": 1})),
            ScriptedResponse::json(400, &serde_json::json!({"error": error_code}).to_string()),
        ]);
        let oauth = XaiOAuth::with_runtime(runtime(fetch, NOW));
        let interaction = TestInteraction::new(vec![PromptAnswer::Error("unexpected prompt".to_owned())]);
        let error = oauth.login(interaction).await.expect_err("device error");
        assert_eq!(error.detail(), Some(&JsString::from(message)));
    }
}

#[tokio::test]
async fn refresh_preserves_an_unrotated_refresh_token_and_defaults_the_lifetime() {
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(
            200,
            &token_body(serde_json::json!({"access_token": "new-access", "refresh_token": "new-refresh"})),
        ),
        ScriptedResponse::json(
            200,
            &token_body(serde_json::json!({"access_token": "newer-access", "refresh_token": null, "expires_in": null})),
        ),
    ]);
    let oauth = XaiOAuth::with_runtime(runtime(fetch.clone(), NOW));

    let rotated = oauth
        .refresh(
            OAuthCredential::new("old-refresh", "old-access", 0.0),
            CancellationToken::new(),
        )
        .await
        .expect("rotated");
    assert_eq!(rotated.access.to_utf8().unwrap(), "new-access");
    assert_eq!(rotated.refresh.to_utf8().unwrap(), "new-refresh");
    assert_eq!(rotated.expires, NOW + 21_600_000.0 - 300_000.0);

    let preserved = oauth
        .refresh(
            OAuthCredential::new("keep-refresh", "old", 0.0),
            CancellationToken::new(),
        )
        .await
        .expect("preserved");
    assert_eq!(preserved.access.to_utf8().unwrap(), "newer-access");
    assert_eq!(preserved.refresh.to_utf8().unwrap(), "keep-refresh");
    assert_eq!(preserved.expires, NOW + 3_600_000.0 - 300_000.0);

    let requests = recorded_requests(&fetch);
    assert_eq!(
        requests[0].body,
        "grant_type=refresh_token&client_id=b1a00492-073a-47ea-816f-4c329264a828&refresh_token=old-refresh"
    );
    assert_eq!(
        requests[1].body,
        "grant_type=refresh_token&client_id=b1a00492-073a-47ea-816f-4c329264a828&refresh_token=keep-refresh"
    );
}

#[tokio::test]
async fn to_auth_and_labels_match_the_source() {
    let oauth = XaiOAuth::with_runtime(runtime(RecordingFetch::new(vec![]), NOW));
    assert_eq!(oauth.name(), "xAI (Grok/X subscription)");
    assert_eq!(oauth.login_label(), Some("Sign in with SuperGrok or X Premium"));
    assert!(oauth.is_subscription());
    let auth = oauth
        .to_auth(OAuthCredential::new("r", "token", 0.0))
        .await
        .expect("auth");
    assert_eq!(auth.api_key.as_ref(), Some(&JsString::from("token")));
}
