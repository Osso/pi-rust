//! Port of `packages/ai/src/providers/huggingface.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `huggingface_provider()`.
pub fn huggingface_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "huggingface",
            name: "Hugging Face",
            base_url: Some("https://router.huggingface.co/v1"),
            auth_name: "Hugging Face token",
            env_vars: &["HF_TOKEN"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
