//! Public OAuth type names exported by frozen ai/src/index.ts.
use std::sync::Arc;

#[test]
fn public_oauth_type_names_support_the_legacy_provider_boundary() {
    let id: pi_ai::OAuthProviderId = "github-copilot".to_owned();
    let provider_id: pi_ai::OAuthProvider = id.clone();
    let info = pi_ai::OAuthProviderInfo {
        id,
        name: "GitHub Copilot".to_owned(),
        available: true,
    };
    let credentials = pi_ai::OAuthCredentials::new("refresh", "access", 0.0);
    let provider: Arc<dyn pi_ai::OAuthProviderInterface> =
        pi_ai::oauth::get_oauth_provider(&provider_id).expect("built-in provider");
    assert_eq!(provider.id(), info.id);
    assert_eq!(provider.get_api_key(&credentials).unwrap(), "access");
}
