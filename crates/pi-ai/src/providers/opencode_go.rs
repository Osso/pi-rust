//! Port of `packages/ai/src/providers/opencode-go.ts`.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::api::openai_completions::open_ai_completions_api;
use crate::api::openai_responses::open_ai_responses_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};
use crate::request::ProviderStreams;

/// `opencodeGoProvider()`.
pub fn opencode_go_provider() -> Arc<dyn Provider> {
    let mut api: BTreeMap<String, Arc<dyn ProviderStreams>> = BTreeMap::new();
    api.insert("anthropic-messages".to_owned(), Arc::new(anthropic_messages_api()));
    api.insert("openai-completions".to_owned(), Arc::new(open_ai_completions_api()));
    api.insert("openai-responses".to_owned(), Arc::new(open_ai_responses_api()));
    env_key_provider(
        ProviderSpec {
            id: "opencode-go",
            name: "OpenCode Go",
            base_url: None,
            auth_name: "OpenCode API key",
            env_vars: &["OPENCODE_API_KEY"],
        },
        ProviderApi::Map(api),
    )
}
