#[path = "../../pi-ai/tests/support/oauth.rs"]
mod oauth_support;
#[path = "support/oracle.rs"]
mod oracle;

use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};
use pi_ai::auth::oauth::openai_codex::OpenAICodexOAuth;
use pi_ai::auth::{AuthError, OAuthAuth};
use pi_ai::oauth::{OAuthCredentials, OAuthLoginCallbacks, OAuthProviderInterface, register_oauth_provider};
use pi_ai::request::{BoxFuture, CallbackError};
use pi_coding_agent::core::auth_storage::{AuthStorage, GetApiKeyOptions};
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde_json::json;
use std::sync::Arc;

const NOW: f64 = 1_750_000_000_000.0;
const INITIAL: &str = r#"{"openai-codex":{"type":"oauth","access":"old","refresh":"old-refresh","expires":0},"keep":{"type":"api_key","key":"literal"}}"#;

struct ScriptedCodex(OpenAICodexOAuth);
impl OAuthProviderInterface for ScriptedCodex {
    fn id(&self) -> &str {
        "openai-codex"
    }
    fn name(&self) -> &str {
        "Codex fixture"
    }
    fn login(&self, _: Arc<dyn OAuthLoginCallbacks>) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        Box::pin(async { Err(AuthError::failed("login is not used by refresh proof")) })
    }
    fn refresh_token(&self, value: OAuthCredentials) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        self.0.refresh(value)
    }
    fn get_api_key(&self, value: &OAuthCredentials) -> Result<JsString, CallbackError> {
        Ok(value.access.clone())
    }
}

fn jwt(payload: &str) -> String {
    format!(
        "e30=.{}.signature",
        pi_js_compat::base64::buffer_to_base64(payload.as_bytes())
    )
}

#[tokio::test]
async fn actual_codex_refresh_wrapper_preserves_email_and_auth_file_key_order() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("auth.json");
    let access = jwt(r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"account-123"}}"#);
    let mut source = oracle::TsOracle::spawn(
        "codex-credential-persistence-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    for id_token in [
        None,
        Some(jwt(r#"{"email":"person@example.test"}"#)),
        Some(jwt(r#"{"email":"","profile":{"email":"profile@example.test"}}"#)),
        Some(jwt(
            r#"{"email":"direct@example.test","profile":{"email":"ignored@example.test"}}"#,
        )),
        Some("invalid.jwt".to_owned()),
        Some(jwt(r#"{"profile":{}}"#)),
        Some(jwt(r#"{"email":"native\ud800@example.test"}"#)),
    ] {
        let mut token = json!({"access_token":access,"refresh_token":"rotated","expires_in":3600});
        if let Some(id_token) = id_token {
            token["id_token"] = json!(id_token);
        }
        std::fs::write(&path, INITIAL).unwrap();
        let expected = source.request(&json!({"id":1,"authPath":path,"tokenResponse":token}));
        assert_eq!(expected["ok"], true, "{expected}");
        std::fs::write(&path, INITIAL).unwrap();
        let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &token.to_string())]);
        register_oauth_provider(Arc::new(ScriptedCodex(OpenAICodexOAuth::with_runtime(
            oauth_support::runtime(fetch, NOW),
        ))));
        let storage = AuthStorage::create(Some(&JsString::from(path.to_str().unwrap()))).unwrap();
        let key = storage
            .get_api_key(
                "openai-codex",
                GetApiKeyOptions {
                    throw_on_refresh_failure: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let actual = json!({
            "key": key.unwrap().to_utf8().unwrap(),
            "credential": js_json::stringify_lossless(&storage.get("openai-codex").unwrap()),
            "file": std::fs::read_to_string(&path).unwrap(),
        });
        assert_eq!(actual, expected["output"], "response={token}");
    }
}

#[tokio::test]
async fn codex_token_email_claim_is_not_dropped() {
    let access = jwt(r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"account-123"}}"#);
    let token = json!({"access_token":access,"refresh_token":"rotated","expires_in":3600,
        "id_token":jwt(r#"{"email":"person@example.test"}"#)});
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &token.to_string())]);
    let provider = OpenAICodexOAuth::with_runtime(oauth_support::runtime(fetch, NOW));
    let credential = provider
        .refresh(OAuthCredentials::new("old-refresh", "old", 0.0))
        .await
        .unwrap();
    assert_eq!(
        credential.extra.get(&JsString::from("email")),
        Some(&JsValue::String("person@example.test".into()))
    );
}

#[test]
fn typed_oauth_credential_roundtrip_keeps_interleaved_known_and_extra_keys() {
    let raw = js_json::parse_lossless(
        r#"{"type":"oauth","access":"a","tenant":"t","refresh":"r","expires":123,"email":"e"}"#,
    )
    .unwrap();
    let value = OAuthCredentials::from_js_value(&raw).unwrap();
    assert_eq!(
        js_json::stringify_lossless(&value.to_js_value()),
        js_json::stringify_lossless(&raw)
    );
    assert_eq!(
        value.extra.get(&JsString::from("tenant")),
        Some(&JsValue::String("t".into()))
    );
}
