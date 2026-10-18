//! Port of `packages/ai/src/providers/opencode.ts`.

use std::sync::Arc;

use indexmap::IndexMap;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::api::google_generative_ai::google_generative_ai_api;
use crate::api::openai_completions::open_ai_completions_api;
use crate::api::openai_responses::open_ai_responses_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};
use crate::request::ProviderStreams;

/// `opencodeProvider()`.
pub fn opencode_provider() -> Arc<dyn Provider> {
    let mut api: IndexMap<String, Arc<dyn ProviderStreams>> = IndexMap::new();
    api.insert("anthropic-messages".to_owned(), Arc::new(anthropic_messages_api()));
    api.insert("google-generative-ai".to_owned(), Arc::new(google_generative_ai_api()));
    api.insert("openai-completions".to_owned(), Arc::new(open_ai_completions_api()));
    api.insert("openai-responses".to_owned(), Arc::new(open_ai_responses_api()));
    env_key_provider(
        ProviderSpec {
            id: "opencode",
            name: "OpenCode Zen",
            base_url: None,
            auth_name: "OpenCode API key",
            env_vars: &["OPENCODE_API_KEY"],
        },
        ProviderApi::Map(api),
    )
}
