use pi_js_compat::js_json::JsString;
use std::time::Duration;

use pi_ai::auth::oauth::radius::{RadiusOAuthOptions, create_radius_oauth};
use pi_ai::auth::{AuthEvent, OAuthAuth, OAuthCredential};
use serde_json::json;

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};
use oauth_support::{PromptAnswer, TestInteraction, http_get, lock_callback_port, recorded_requests, runtime};
const NOW: f64 = 1_750_000_000_000.0;
const GATEWAY: &str = "https://radius.example";
const UUID: &str = "00000000-0000-4000-8000-000000000000";

fn oauth(fetch: std::sync::Arc<RecordingFetch>) -> impl OAuthAuth {
    create_radius_oauth(RadiusOAuthOptions {
        name: "Radius".to_owned(),
        gateway: GATEWAY.into(),
    })
    .with_runtime(runtime(fetch, NOW))
}

#[tokio::test]
async fn device_login_uses_gateway_endpoints_and_keeps_the_scope() {
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(
            200,
            &json!({
                "device_code": "device-code",
                "user_code": "ABCD-1234",
                "verification_uri": "https://radius-ui.example/pair",
                "expires_in": 600,
                "interval": 5
            })
            .to_string(),
        ),
        ScriptedResponse::json(
            200,
            &json!({
                "access_token": "access-token",
                "refresh_token": "refresh-token",
                "expires_in": 3600,
                "scope": "gateway offline_access"
            })
            .to_string(),
        ),
    ]);
    let oauth = oauth(fetch.clone());
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("device-code".to_owned())]);

    let credential = oauth.login(interaction.clone()).await.expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), "access-token");
    assert_eq!(credential.refresh.to_utf8().unwrap(), "refresh-token");
    assert_eq!(credential.expires, NOW + 3_600_000.0 - 60_000.0);
    assert_eq!(
        credential.extra.get(&pi_js_compat::js_json::JsString::from("scope")),
        Some(&pi_js_compat::js_json::JsValue::from(&json!("gateway offline_access")))
    );
    assert_eq!(
        interaction.events()[0],
        AuthEvent::DeviceCode {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://radius-ui.example/pair".into(),
            interval_seconds: Some(5.0),
            expires_in_seconds: Some(600.0),
        }
    );

    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url, format!("{GATEWAY}/v1/oauth/device"));
    assert_eq!(requests[0].body, "client_id=pi-gateway&scope=gateway+offline_access");
    assert_eq!(requests[1].url, format!("{GATEWAY}/v1/oauth/token"));
    assert_eq!(
        requests[1].body,
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id=pi-gateway&device_code=device-code"
    );
}

#[tokio::test]
async fn refresh_uses_the_gateway_without_discovery() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        &json!({"access_token": "new-access", "refresh_token": "new-refresh", "expires_in": 3600}).to_string(),
    )]);
    let oauth = oauth(fetch.clone());
    let refreshed = oauth
        .refresh(OAuthCredential::new("old-refresh", "old-access", 0.0))
        .await
        .expect("refreshed");
    assert_eq!(refreshed.access.to_utf8().unwrap(), "new-access");
    assert_eq!(refreshed.refresh.to_utf8().unwrap(), "new-refresh");
    assert_eq!(
        recorded_requests(&fetch)[0].body,
        "grant_type=refresh_token&client_id=pi-gateway&refresh_token=old-refresh"
    );
}

#[tokio::test]
async fn browser_discovery_requires_the_authorization_endpoint() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        r#"{"issuer":"https://radius-ui.example"}"#,
    )]);
    let oauth = oauth(fetch.clone());
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("browser".to_owned())]);
    let error = oauth.login(interaction).await.expect_err("invalid discovery");
    assert_eq!(
        error.detail(),
        Some(&JsString::from(
            "Invalid Radius OAuth config from https://radius.example"
        ))
    );
    assert_eq!(recorded_requests(&fetch).len(), 1);
}

#[tokio::test]
async fn browser_login_runs_the_callback_and_token_exchange() {
    let _port = lock_callback_port().await;
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(
            200,
            r#"{"authorizationEndpoint":"https://radius-ui.example/authorize"}"#,
        ),
        ScriptedResponse::json(
            200,
            &json!({"access_token": "access-token", "refresh_token": "refresh-token", "expires_in": 60}).to_string(),
        ),
    ]);
    let oauth = oauth(fetch.clone());
    let interaction = TestInteraction::new(vec![
        PromptAnswer::Text("browser".to_owned()),
        PromptAnswer::Error("unexpected prompt".to_owned()),
    ]);
    let login = tokio::spawn({
        let interaction = interaction.clone();
        async move { oauth.login(interaction).await }
    });

    let mut auth_url = None;
    for _ in 0..200 {
        if let Some(AuthEvent::AuthUrl { url, .. }) = interaction
            .events()
            .into_iter()
            .find(|event| matches!(event, AuthEvent::AuthUrl { .. }))
        {
            auth_url = Some(url);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let auth_url = auth_url.expect("auth_url");
    let parsed = url::Url::parse(&pi_js_compat::js_json::encode_node_utf8(&auth_url)).expect("authorize URL");
    assert_eq!(parsed.origin().ascii_serialization(), "https://radius-ui.example");
    assert_eq!(parsed.path(), "/authorize");
    let params: std::collections::BTreeMap<_, _> = parsed.query_pairs().into_owned().collect();
    assert_eq!(params.get("client_id").map(String::as_str), Some("pi-gateway"));
    assert_eq!(
        params.get("redirect_uri").map(String::as_str),
        Some("http://127.0.0.1:1456/oauth/callback")
    );
    assert_eq!(params.get("scope").map(String::as_str), Some("gateway offline_access"));
    assert_eq!(params.get("handoff").map(String::as_str), Some("url"));
    assert_eq!(params.get("state").map(String::as_str), Some(UUID));

    let (status, body) = http_get(&format!(
        "http://127.0.0.1:1456/oauth/callback?code=callback-code&state={UUID}"
    ))
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Signed in to Radius. You may now close this page."));

    let credential = login.await.expect("task").expect("credential");
    assert_eq!(credential.access.to_utf8().unwrap(), "access-token");
    let requests = recorded_requests(&fetch);
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].body,
        "grant_type=authorization_code&client_id=pi-gateway&redirect_uri=http%3A%2F%2F127.0.0.1%3A1456%2Foauth%2Fcallback&code=callback-code&code_verifier=AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA"
    );
}

#[tokio::test]
async fn device_denials_and_unknown_methods_have_source_messages() {
    for (error_code, message) in [
        ("expired_token", "Device authorization expired."),
        ("access_denied", "Device authorization was denied."),
    ] {
        let fetch = RecordingFetch::new(vec![
            ScriptedResponse::json(
                200,
                &json!({
                    "device_code": "device-code",
                    "user_code": "ABCD-1234",
                    "verification_uri": "https://radius-ui.example/pair",
                    "expires_in": 600,
                    "interval": 1
                })
                .to_string(),
            ),
            ScriptedResponse::json(400, &json!({"error": error_code}).to_string()),
        ]);
        let oauth = oauth(fetch);
        let interaction = TestInteraction::new(vec![PromptAnswer::Text("device-code".to_owned())]);
        let error = oauth.login(interaction).await.expect_err("device error");
        assert_eq!(error.detail(), Some(&JsString::from(message)));
    }

    let oauth = oauth(RecordingFetch::new(vec![]));
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("carrier-pigeon".to_owned())]);
    let error = oauth.login(interaction).await.expect_err("unknown method");
    assert_eq!(
        error.detail(),
        Some(&JsString::from("Unknown Radius sign-in method: carrier-pigeon"))
    );
}

#[tokio::test]
async fn to_auth_uses_the_access_token() {
    let oauth = oauth(RecordingFetch::new(vec![]));
    assert_eq!(oauth.name(), "Radius");
    let auth = oauth
        .to_auth(OAuthCredential::new("r", "token", 0.0))
        .await
        .expect("auth");
    assert_eq!(auth.api_key.as_ref(), Some(&JsString::from("token")));
}
