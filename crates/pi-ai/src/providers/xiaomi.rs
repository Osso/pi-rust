//! Port of `packages/ai/src/providers/xiaomi.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `xiaomi_provider()`.
pub fn xiaomi_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "xiaomi",
            name: "Xiaomi",
            base_url: Some("https://api.xiaomimimo.com/v1"),
            auth_name: "Xiaomi API key",
            env_vars: &["XIAOMI_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
