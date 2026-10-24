//! Port of `packages/ai/src/providers/kimi-coding.ts`.

use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::auth::{ProviderAuth, env_api_key_auth};
use crate::models::Provider;
use crate::providers::{CustomProviderSpec, custom_provider, single_api};

/// `kimiCodingProvider()`.
pub fn kimi_coding_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "kimi-coding",
            name: "Kimi For Coding",
            base_url: Some("https://api.kimi.com/coding"),
            auth: ProviderAuth::api_key(Arc::new(env_api_key_auth("Kimi API key", &["KIMI_API_KEY"]))),
        },
        single_api(anthropic_messages_api()),
    )
}
