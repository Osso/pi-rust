//! Port of `packages/ai/src/providers/ant-ling.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `ant_ling_provider()`.
pub fn ant_ling_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "ant-ling",
            name: "Ant Ling",
            base_url: Some("https://api.ant-ling.com/v1"),
            auth_name: "Ant Ling API key",
            env_vars: &["ANT_LING_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
