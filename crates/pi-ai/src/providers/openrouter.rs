//! Port of `packages/ai/src/providers/openrouter.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::auth::oauth::load::load_openrouter_oauth;
use crate::auth::{LazyOAuthInput, ProviderAuth, env_api_key_auth, lazy_oauth};
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, oauth_loader, single_api};

/// `openrouterProvider()`.
pub fn openrouter_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "openrouter",
            name: "OpenRouter",
            base_url: Some("https://openrouter.ai/api/v1"),
            auth: ProviderAuth {
                api_key: Some(Arc::new(env_api_key_auth(
                    "OpenRouter API key",
                    &["OPENROUTER_API_KEY"],
                ))),
                oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
                    name: "OpenRouter OAuth".to_owned(),
                    is_subscription: None,
                    login_label: Some("Sign in with OpenRouter".to_owned()),
                    load: oauth_loader(load_openrouter_oauth),
                }))),
            },
            filter_models: None,
        },
        single_api(open_ai_completions_api()),
    )
}
