//! Port of `packages/ai/src/providers/groq.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `groq_provider()`.
pub fn groq_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "groq",
            name: "Groq",
            base_url: Some("https://api.groq.com/openai/v1"),
            auth_name: "Groq API key",
            env_vars: &["GROQ_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
