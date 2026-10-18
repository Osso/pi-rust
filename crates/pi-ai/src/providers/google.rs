//! Port of `packages/ai/src/providers/google.ts`.

use std::sync::Arc;

use crate::api::google_generative_ai::google_generative_ai_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `google_provider()`.
pub fn google_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "google",
            name: "Google",
            base_url: Some("https://generativelanguage.googleapis.com/v1beta"),
            auth_name: "Gemini API key",
            env_vars: &["GEMINI_API_KEY"],
        },
        ProviderApi::Single(Arc::new(google_generative_ai_api())),
    )
}
