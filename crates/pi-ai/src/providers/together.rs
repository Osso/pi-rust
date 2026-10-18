//! Port of `packages/ai/src/providers/together.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `together_provider()`.
pub fn together_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "together",
            name: "Together",
            base_url: Some("https://api.together.ai/v1"),
            auth_name: "Together API key",
            env_vars: &["TOGETHER_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
