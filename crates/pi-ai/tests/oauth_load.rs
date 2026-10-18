use std::sync::Arc;

use pi_ai::auth::oauth::load::{
    OAuthFlowLoaders, load_anthropic_oauth, load_github_copilot_oauth, load_kimi_coding_oauth, load_openai_codex_oauth,
    load_openrouter_oauth, load_radius_oauth, load_xai_oauth, register_bundled_oauth_flow_loaders,
};
use pi_ai::auth::oauth::radius::RadiusOAuthOptions;
use pi_ai::auth::{AuthError, AuthInteraction, ModelAuth, OAuthAuth, OAuthCredential};
use pi_ai::bun_oauth::register_bun_oauth_flows;
use tokio_util::sync::CancellationToken;

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::TestInteraction;

struct StubOAuth(&'static str);

impl OAuthAuth for StubOAuth {
    fn name(&self) -> &str {
        self.0
    }

    fn login(
        &self,
        _interaction: Arc<dyn AuthInteraction>,
    ) -> pi_ai::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async { Err(AuthError::failed("stub")) })
    }

    fn refresh(
        &self,
        _credential: OAuthCredential,
        _signal: CancellationToken,
    ) -> pi_ai::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async { Err(AuthError::failed("stub")) })
    }

    fn to_auth(
        &self,
        _credential: OAuthCredential,
    ) -> pi_ai::request::BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async { Err(AuthError::failed("stub")) })
    }
}

fn stub(name: &'static str) -> Arc<dyn OAuthAuth> {
    Arc::new(StubOAuth(name))
}

#[tokio::test]
async fn loader_registry_defaults_overrides_and_bun_registration() {
    assert_eq!(load_anthropic_oauth().name(), "Anthropic (Claude Pro/Max)");
    assert_eq!(load_openai_codex_oauth().name(), "OpenAI (ChatGPT Plus/Pro)");
    assert_eq!(load_openrouter_oauth().name(), "OpenRouter OAuth");
    assert_eq!(load_kimi_coding_oauth().name(), "Kimi Code (subscription)");
    assert_eq!(load_xai_oauth().name(), "xAI (Grok/X subscription)");
    assert_eq!(
        load_radius_oauth(RadiusOAuthOptions {
            name: "Radius".to_owned(),
            gateway: "radius.example".to_owned(),
        })
        .name(),
        "Radius"
    );
    let github = load_github_copilot_oauth();
    assert_eq!(github.name(), "GitHub Copilot");
    let error = github
        .login(TestInteraction::new(vec![]))
        .await
        .expect_err("unavailable");
    assert_eq!(error.detail(), Some("GitHub Copilot OAuth is not ported yet"));

    register_bundled_oauth_flow_loaders(OAuthFlowLoaders {
        anthropic: Arc::new(|| stub("custom-anthropic")),
        openai_codex: Arc::new(|| stub("custom-openai")),
        github_copilot: Arc::new(|| stub("custom-github")),
        openrouter: Arc::new(|| stub("custom-openrouter")),
        kimi_coding: Arc::new(|| stub("custom-kimi")),
        xai: Arc::new(|| stub("custom-xai")),
        radius: Arc::new(|options| {
            stub(if options.name == "Radius" {
                "custom-radius"
            } else {
                "other"
            })
        }),
    });
    assert_eq!(load_anthropic_oauth().name(), "custom-anthropic");
    assert_eq!(load_openai_codex_oauth().name(), "custom-openai");
    assert_eq!(load_github_copilot_oauth().name(), "custom-github");
    assert_eq!(load_openrouter_oauth().name(), "custom-openrouter");
    assert_eq!(load_kimi_coding_oauth().name(), "custom-kimi");
    assert_eq!(load_xai_oauth().name(), "custom-xai");
    assert_eq!(
        load_radius_oauth(RadiusOAuthOptions {
            name: "Radius".to_owned(),
            gateway: "https://radius.example".to_owned(),
        })
        .name(),
        "custom-radius"
    );

    register_bun_oauth_flows();
    assert_eq!(load_anthropic_oauth().name(), "Anthropic (Claude Pro/Max)");
    assert_eq!(load_openai_codex_oauth().name(), "OpenAI (ChatGPT Plus/Pro)");
    assert_eq!(load_openrouter_oauth().name(), "OpenRouter OAuth");
    assert_eq!(load_kimi_coding_oauth().name(), "Kimi Code (subscription)");
    assert_eq!(load_xai_oauth().name(), "xAI (Grok/X subscription)");
    assert_eq!(load_github_copilot_oauth().name(), "GitHub Copilot");
}
