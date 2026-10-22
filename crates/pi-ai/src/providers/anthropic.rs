//! Port of `packages/ai/src/providers/anthropic.ts`.

use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::auth::oauth::load::load_anthropic_oauth;
use crate::auth::{
    ApiKeyAuth, ApiKeyCredential, ApiKeyResolveInput, AuthError, AuthInteraction, AuthPrompt, AuthPromptKind,
    AuthResult, LazyOAuthInput, ModelAuth, ProviderAuth, lazy_oauth,
};
use crate::env_api_keys::{ANTHROPIC_API_KEY_ENV, ANTHROPIC_AUTH_TOKEN_ENV, ANTHROPIC_OAUTH_TOKEN_ENV};
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, oauth_loader, single_api};
use crate::request::BoxFuture;
use crate::types::ProviderHeaders;

/// `anthropicApiKeyAuth()`.
struct AnthropicApiKeyAuth;

impl ApiKeyAuth for AnthropicApiKeyAuth {
    fn name(&self) -> &str {
        "Anthropic API key"
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
        Some(Box::pin(async move {
            let signal = interaction.signal();
            check_aborted(&signal)?;
            let key = interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::Secret {
                        message: "Enter Anthropic API key".into(),
                        placeholder: None,
                    },
                })
                .await
                .map_err(AuthError::Failed)?;
            check_aborted(&signal)?;
            Ok(ApiKeyCredential {
                key: Some(key),
                env: None,
            })
        }))
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move {
            check_aborted(&input.signal)?;
            if let Some(key) = input.credential.as_ref().and_then(|credential| credential.key.as_ref()) {
                return Ok(Some(AuthResult {
                    auth: ModelAuth {
                        api_key: Some(key.clone()),
                        headers: None,
                        base_url: None,
                    },
                    env: input.credential.and_then(|credential| credential.env),
                    source: Some("stored credential".into()),
                }));
            }

            let auth_token = input.ctx.env(ANTHROPIC_AUTH_TOKEN_ENV).await;
            check_aborted(&input.signal)?;
            if let Some(auth_token) = auth_token {
                let mut headers = ProviderHeaders::new();
                headers.insert("Authorization".into(), Some(format!("Bearer {auth_token}")));
                return Ok(Some(AuthResult {
                    auth: ModelAuth {
                        api_key: None,
                        headers: Some(headers),
                        base_url: None,
                    },
                    env: None,
                    source: Some(ANTHROPIC_AUTH_TOKEN_ENV.into()),
                }));
            }

            for env_var in [ANTHROPIC_OAUTH_TOKEN_ENV, ANTHROPIC_API_KEY_ENV] {
                let api_key = input.ctx.env(env_var).await;
                check_aborted(&input.signal)?;
                if let Some(api_key) = api_key {
                    return Ok(Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(api_key.into()),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some(env_var.into()),
                    }));
                }
            }
            Ok(None)
        })
    }
}

fn check_aborted(signal: &tokio_util::sync::CancellationToken) -> Result<(), AuthError> {
    if signal.is_cancelled() {
        Err(AuthError::Aborted)
    } else {
        Ok(())
    }
}

/// `anthropicProvider()`.
pub fn anthropic_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "anthropic",
            name: "Anthropic",
            base_url: Some("https://api.anthropic.com"),
            auth: ProviderAuth {
                api_key: Some(Arc::new(AnthropicApiKeyAuth)),
                oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
                    name: "Anthropic (Claude Pro/Max)".into(),
                    is_subscription: Some(true),
                    login_label: None,
                    load: oauth_loader(load_anthropic_oauth),
                }))),
            },
            filter_models: None,
        },
        single_api(anthropic_messages_api()),
    )
}
