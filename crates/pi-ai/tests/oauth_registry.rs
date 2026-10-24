//! Frozen registry/result provenance; fake providers only, no network.
use pi_ai::{
    auth::{AuthError, OAuthCredential},
    oauth::*,
    request::BoxFuture,
};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Fake(Arc<AtomicUsize>);
impl OAuthProviderInterface for Fake {
    fn id(&self) -> &str {
        "phase1-fake"
    }
    fn name(&self) -> &str {
        "Synthetic"
    }
    fn login(
        &self,
        _callbacks: Arc<dyn OAuthLoginCallbacks>,
    ) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        Box::pin(async { Err(AuthError::failed("unused")) })
    }
    fn refresh_token(&self, c: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            assert_eq!(c.refresh.to_utf8().unwrap(), "A");
            Ok(OAuthCredential::new("B", "new", 4_000_000_000_000.0))
        })
    }
    fn get_api_key(
        &self,
        c: &OAuthCredentials,
    ) -> Result<pi_js_compat::js_json::JsString, pi_ai::request::CallbackError> {
        Ok(c.access.clone())
    }
}
#[tokio::test]
async fn registry_helpers_preserve_result_provenance_and_builtin_order() {
    reset_oauth_providers();
    assert_eq!(
        get_oauth_providers()
            .iter()
            .map(|p| p.id().to_owned())
            .collect::<Vec<_>>(),
        [
            "anthropic",
            "github-copilot",
            "openai-codex",
            "openai-codex-gc",
            "openai-codex-team"
        ]
    );
    let count = Arc::new(AtomicUsize::new(0));
    register_oauth_provider(Arc::new(Fake(count.clone())));
    assert!(
        get_oauth_api_key("phase1-fake", &HashMap::new())
            .await
            .unwrap()
            .is_none()
    );
    let creds = HashMap::from([("phase1-fake".to_owned(), OAuthCredential::new("A", "old", 0.0))]);
    let result = get_oauth_api_key("phase1-fake", &creds).await.unwrap().unwrap();
    assert_eq!(result.api_key.to_utf8().unwrap(), "new");
    assert_eq!(result.new_credentials.refresh.to_utf8().unwrap(), "B");
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let creds = HashMap::from([("phase1-fake".to_owned(), result.new_credentials)]);
    assert_eq!(
        get_oauth_api_key("phase1-fake", &creds)
            .await
            .unwrap()
            .unwrap()
            .api_key
            .to_utf8()
            .unwrap(),
        "new"
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    unregister_oauth_provider("phase1-fake");
    assert!(get_oauth_provider("phase1-fake").is_none());
    assert!(
        refresh_oauth_token("phase1-fake", OAuthCredential::new("A", "old", 0.0))
            .await
            .unwrap_err()
            .to_string()
            .contains("Unknown OAuth provider: phase1-fake")
    );
}

#[path = "support/oauth.rs"]
mod support;
#[tokio::test]
async fn builtin_response_handoff_preserves_utf16_tokens() {
    use pi_ai::auth::{OAuthAuth, oauth::anthropic::AnthropicOAuth};
    use support::recording_fetch::{RecordingFetch, ScriptedResponse};
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
        200,
        r#"{"access_token":"a\ud800","refresh_token":"r\udfff","expires_in":3600}"#,
    )]);
    let provider = AnthropicOAuth::with_runtime(support::runtime(fetch.clone(), 0.0));
    let credential = provider
        .refresh(OAuthCredential::new(
            pi_js_compat::js_json::JsString::from_utf16(vec![65, 0xdfff]),
            "old",
            0.0,
        ))
        .await
        .unwrap();
    assert_eq!(credential.refresh.units(), [114, 0xdfff]);
    assert_eq!(credential.access.units(), [97, 0xd800]);
    let requests = fetch.requests();
    let body = std::str::from_utf8(requests[0].body.as_ref().unwrap()).unwrap();
    let sent = pi_js_compat::js_json::parse_lossless(body).unwrap();
    assert_eq!(
        sent.get("refresh_token").unwrap().as_string().unwrap().units(),
        [65, 0xdfff]
    );
}

#[test]
fn outbound_json_form_and_header_match_node_without_mutating_tokens() {
    use pi_ai::auth::oauth::support::{form_encode, json_request, to_form_usv_string};
    use pi_js_compat::js_json::{JsObject, JsString, JsValue};
    for units in [vec![65, 0xdfff], vec![65, 10], vec![65, 255], vec![0xd83d, 0xde00]] {
        let output = std::process::Command::new("node").args(["-e",r#"const units=JSON.parse(process.argv[1]);const text=String.fromCharCode(...units);let header=null;try{new Headers({authorization:text})}catch(e){header={name:e.name,message:e.message,code:e.code}};console.log(JSON.stringify({json:JSON.stringify({refresh_token:text}),form:new URLSearchParams({refresh_token:text}).toString(),header}));"#,&serde_json::to_string(&units).unwrap()]).output().unwrap();
        assert!(output.status.success());
        let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let token = JsString::from_utf16(units.clone());
        let body = JsValue::Object(JsObject::from_iter([(
            "refresh_token".into(),
            JsValue::String(token.clone()),
        )]));
        let request = json_request("http://localhost.invalid", body, false);
        assert_eq!(String::from_utf8(request.body.unwrap()).unwrap(), source["json"]);
        assert_eq!(
            form_encode(&[("refresh_token", &to_form_usv_string(&token))]),
            source["form"]
        );
        let credential = OAuthCredential::new(token, "unused", 0.0);
        match pi_ai::request::header_value(&credential.refresh) {
            Ok(_) => assert_eq!(source["header"], serde_json::Value::Null),
            Err(error) => {
                assert_eq!(error.name, source["header"]["name"].as_str().unwrap());
                assert_eq!(error.message, source["header"]["message"].as_str().unwrap());
                assert_eq!(error.code.as_deref(), source["header"]["code"].as_str());
            }
        }
        assert_eq!(credential.refresh.units(), units);
    }
}
