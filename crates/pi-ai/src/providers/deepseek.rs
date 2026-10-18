//! Port of `packages/ai/src/providers/deepseek.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `deepseek_provider()`.
pub fn deepseek_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "deepseek",
            name: "DeepSeek",
            base_url: Some("https://api.deepseek.com"),
            auth_name: "DeepSeek API key",
            env_vars: &["DEEPSEEK_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
