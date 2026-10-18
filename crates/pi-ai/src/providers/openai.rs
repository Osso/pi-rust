//! Port of `packages/ai/src/providers/openai.ts`.

use std::sync::Arc;

use crate::api::openai_responses::open_ai_responses_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `openai_provider()`.
pub fn openai_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "openai",
            name: "OpenAI",
            base_url: Some("https://api.openai.com/v1"),
            auth_name: "OpenAI API key",
            env_vars: &["OPENAI_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_responses_api())),
    )
}
