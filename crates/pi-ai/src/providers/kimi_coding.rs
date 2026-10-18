//! Port of `packages/ai/src/providers/kimi-coding.ts`.

use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::auth::oauth::load::load_kimi_coding_oauth;
use crate::auth::{LazyOAuthInput, ProviderAuth, env_api_key_auth, lazy_oauth};
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, oauth_loader, single_api};

/// `kimiCodingProvider()`.
pub fn kimi_coding_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "kimi-coding",
            name: "Kimi For Coding",
            base_url: Some("https://api.kimi.com/coding"),
            auth: ProviderAuth {
                api_key: Some(Arc::new(env_api_key_auth("Kimi API key", &["KIMI_API_KEY"]))),
                oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
                    name: "Kimi Code (subscription)".to_owned(),
                    is_subscription: Some(true),
                    login_label: Some("Sign in with Kimi Code".to_owned()),
                    load: oauth_loader(load_kimi_coding_oauth),
                }))),
            },
            filter_models: None,
        },
        single_api(anthropic_messages_api()),
    )
}
