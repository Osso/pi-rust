//! Port of `packages/ai/src/providers/mistral.ts`.

use std::sync::Arc;

use crate::api::mistral_conversations::mistral_conversations_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `mistral_provider()`.
pub fn mistral_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "mistral",
            name: "Mistral",
            base_url: Some("https://api.mistral.ai"),
            auth_name: "Mistral API key",
            env_vars: &["MISTRAL_API_KEY"],
        },
        ProviderApi::Single(Arc::new(mistral_conversations_api())),
    )
}
