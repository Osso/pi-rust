//! Port of `packages/ai/src/providers/nvidia.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `nvidia_provider()`.
pub fn nvidia_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "nvidia",
            name: "NVIDIA",
            base_url: Some("https://integrate.api.nvidia.com/v1"),
            auth_name: "NVIDIA API key",
            env_vars: &["NVIDIA_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
