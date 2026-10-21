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
    fn get_api_key(&self, c: &OAuthCredentials) -> pi_js_compat::js_json::JsString {
        c.access.clone()
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
