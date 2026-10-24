//! Port of `packages/ai/src/providers/anthropic.ts`.

use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::auth::oauth::load::load_anthropic_oauth;
use crate::auth::{LazyOAuthInput, ProviderAuth, env_api_key_auth, lazy_oauth};
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, oauth_loader, single_api};

/// `anthropicProvider()`.
pub fn anthropic_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "anthropic",
            name: "Anthropic",
            base_url: Some("https://api.anthropic.com"),
            auth: ProviderAuth {
                api_key: Some(Arc::new(env_api_key_auth(
                    "Anthropic API key",
                    &["ANTHROPIC_OAUTH_TOKEN", "ANTHROPIC_API_KEY"],
                ))),
                oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
                    name: "Anthropic (Claude Pro/Max)".into(),
                    is_subscription: None,
                    login_label: None,
                    load: oauth_loader(load_anthropic_oauth),
                }))),
            },
        },
        single_api(anthropic_messages_api()),
    )
}
