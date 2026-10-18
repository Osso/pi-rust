//! Port of `packages/ai/src/providers/cerebras.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `cerebras_provider()`.
pub fn cerebras_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "cerebras",
            name: "Cerebras",
            base_url: Some("https://api.cerebras.ai/v1"),
            auth_name: "Cerebras API key",
            env_vars: &["CEREBRAS_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
