//! Port of `packages/ai/src/providers/baseten.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `baseten_provider()`.
pub fn baseten_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "baseten",
            name: "Baseten",
            base_url: Some("https://inference.baseten.co/v1"),
            auth_name: "Baseten API key",
            env_vars: &["BASETEN_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
