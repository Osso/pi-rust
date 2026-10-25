//! Public OAuth type names exported by frozen ai/src/index.ts.
use std::sync::Arc;

#[tokio::test]
async fn direct_helpers_and_provider_exports_use_the_frozen_public_surface() {
    use pi_ai::oauth::*;
    use pi_ai::request::{BoxFuture, CallbackError};
    use pi_js_compat::js_json::JsString;
    struct Callbacks;
    impl OAuthLoginCallbacks for Callbacks {
        fn on_auth(&self, _: OAuthAuthInfo) -> Result<(), CallbackError> {
            Err(CallbackError::new("notify stop"))
        }
        fn on_device_code(&self, _: OAuthDeviceCodeInfo) -> Result<(), CallbackError> {
            Ok(())
        }
        fn on_prompt(&self, _: OAuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
            Box::pin(async { Err(CallbackError::new("prompt stop")) })
        }
        fn on_select(&self, _: OAuthSelectPrompt) -> BoxFuture<'_, Result<Option<JsString>, CallbackError>> {
            Box::pin(async { Ok(Some(JsString::default())) })
        }
    }
    assert_eq!(OPENAI_CODEX_BROWSER_LOGIN_METHOD, "browser");
    assert_eq!(OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD, "device_code");
    assert_eq!(anthropic_oauth_provider().id(), "anthropic");
    assert_eq!(github_copilot_oauth_provider().id(), "github-copilot");
    assert_eq!(openai_codex_oauth_provider().id(), "openai-codex");
    assert_eq!(openai_codex_gc_oauth_provider().id(), "openai-codex-gc");
    assert_eq!(openai_codex_team_oauth_provider().id(), "openai-codex-team");
    assert_eq!(
        normalize_domain(&"https://Company.GHE.com/path".into()).as_deref(),
        Some("company.ghe.com")
    );
    assert_eq!(
        get_github_copilot_base_url(None, None),
        "https://api.individual.githubcopilot.com"
    );
    assert_eq!(
        login_github_copilot(Arc::new(Callbacks)).await.unwrap_err().detail(),
        Some(&JsString::from("prompt stop"))
    );
    assert_eq!(
        login_anthropic(Arc::new(Callbacks)).await.unwrap_err().detail(),
        Some(&JsString::from("notify stop"))
    );
    for provider in [
        openai_codex_oauth_provider(),
        openai_codex_gc_oauth_provider(),
        openai_codex_team_oauth_provider(),
    ] {
        assert_eq!(
            provider.login(Arc::new(Callbacks)).await.unwrap_err().detail(),
            Some(&JsString::from("Login cancelled"))
        );
    }
    // Link every direct async entry without executing HTTP or waiting for a callback.
    let _ = (
        login_anthropic,
        refresh_anthropic_token,
        refresh_github_copilot_token,
        login_openai_codex,
        login_openai_codex_device_code,
        refresh_openai_codex_token,
    );
}

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
