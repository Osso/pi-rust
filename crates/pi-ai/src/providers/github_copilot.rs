//! Port of `packages/ai/src/providers/github-copilot.ts`.

use std::sync::Arc;

use indexmap::IndexMap;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::api::openai_completions::open_ai_completions_api;
use crate::api::openai_responses::open_ai_responses_api;
use crate::auth::oauth::load::load_github_copilot_oauth;
use crate::auth::{LazyOAuthInput, ProviderAuth, env_api_key_auth, lazy_oauth};
use crate::models::{Provider, ProviderApi};
use crate::providers::{CustomProviderSpec, custom_provider, oauth_loader};
use crate::request::ProviderStreams;

/// `githubCopilotProvider()`.
pub fn github_copilot_provider() -> Arc<dyn Provider> {
    let mut api: IndexMap<String, Arc<dyn ProviderStreams>> = IndexMap::new();
    api.insert("anthropic-messages".to_owned(), Arc::new(anthropic_messages_api()));
    api.insert("openai-completions".to_owned(), Arc::new(open_ai_completions_api()));
    api.insert("openai-responses".to_owned(), Arc::new(open_ai_responses_api()));
    custom_provider(
        CustomProviderSpec {
            id: "github-copilot",
            name: "GitHub Copilot",
            base_url: Some("https://api.individual.githubcopilot.com"),
            auth: ProviderAuth {
                api_key: Some(Arc::new(env_api_key_auth(
                    "GitHub Copilot token",
                    &["COPILOT_GITHUB_TOKEN"],
                ))),
                oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
                    name: "GitHub Copilot".to_owned(),
                    is_subscription: None,
                    login_label: None,
                    load: oauth_loader(load_github_copilot_oauth),
                }))),
            },
            filter_models: None,
        },
        ProviderApi::Map(api),
    )
}
