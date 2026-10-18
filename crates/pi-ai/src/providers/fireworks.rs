//! Port of `packages/ai/src/providers/fireworks.ts`.

use indexmap::IndexMap;
use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};
use crate::request::ProviderStreams;

/// `fireworksProvider()`.
pub fn fireworks_provider() -> Arc<dyn Provider> {
    let mut api: IndexMap<String, Arc<dyn ProviderStreams>> = IndexMap::new();
    api.insert("anthropic-messages".to_owned(), Arc::new(anthropic_messages_api()));
    api.insert("openai-completions".to_owned(), Arc::new(open_ai_completions_api()));
    env_key_provider(
        ProviderSpec {
            id: "fireworks",
            name: "Fireworks",
            base_url: Some("https://api.fireworks.ai/inference"),
            auth_name: "Fireworks API key",
            env_vars: &["FIREWORKS_API_KEY"],
        },
        ProviderApi::Map(api),
    )
}
