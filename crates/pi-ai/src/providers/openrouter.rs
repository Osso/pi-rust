//! Port of `packages/ai/src/providers/openrouter.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::auth::{ProviderAuth, env_api_key_auth};
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, single_api};

/// `openrouterProvider()`.
pub fn openrouter_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "openrouter",
            name: "OpenRouter",
            base_url: Some("https://openrouter.ai/api/v1"),
            auth: ProviderAuth::api_key(Arc::new(env_api_key_auth(
                "OpenRouter API key",
                &["OPENROUTER_API_KEY"],
            ))),
            filter_models: None,
        },
        single_api(open_ai_completions_api()),
    )
}
