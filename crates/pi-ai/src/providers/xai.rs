//! Port of `packages/ai/src/providers/xai.ts`.

use std::sync::Arc;

use indexmap::IndexMap;

use crate::api::openai_completions::open_ai_completions_api;
use crate::api::openai_responses::open_ai_responses_api;
use crate::auth::oauth::load::load_xai_oauth;
use crate::auth::{LazyOAuthInput, ProviderAuth, env_api_key_auth, lazy_oauth};
use crate::models::{Provider, ProviderApi};
use crate::providers::{CustomProviderSpec, custom_provider, oauth_loader};
use crate::request::ProviderStreams;

/// `xaiProvider()`.
pub fn xai_provider() -> Arc<dyn Provider> {
    let mut api: IndexMap<String, Arc<dyn ProviderStreams>> = IndexMap::new();
    api.insert("openai-completions".to_owned(), Arc::new(open_ai_completions_api()));
    api.insert("openai-responses".to_owned(), Arc::new(open_ai_responses_api()));
    custom_provider(
        CustomProviderSpec {
            id: "xai",
            name: "xAI",
            base_url: Some("https://api.x.ai/v1"),
            auth: ProviderAuth {
                api_key: Some(Arc::new(env_api_key_auth("xAI API key", &["XAI_API_KEY"]))),
                oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
                    name: "xAI (Grok/X subscription)".to_owned(),
                    is_subscription: Some(true),
                    login_label: Some("Sign in with SuperGrok or X Premium".to_owned()),
                    load: oauth_loader(load_xai_oauth),
                }))),
            },
            filter_models: None,
        },
        ProviderApi::Map(api),
    )
}
