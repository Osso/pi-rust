//! Port of `packages/ai/src/providers/zai.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `zai_provider()`.
pub fn zai_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "zai",
            name: "Z.AI",
            base_url: Some("https://api.z.ai/api/coding/paas/v4"),
            auth_name: "Z.AI API key",
            env_vars: &["ZAI_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
