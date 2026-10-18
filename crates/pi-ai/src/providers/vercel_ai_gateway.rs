//! Port of `packages/ai/src/providers/vercel-ai-gateway.ts`.

use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `vercel_ai_gateway_provider()`.
pub fn vercel_ai_gateway_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "vercel-ai-gateway",
            name: "Vercel AI Gateway",
            base_url: Some("https://ai-gateway.vercel.sh"),
            auth_name: "Vercel AI Gateway API key",
            env_vars: &["AI_GATEWAY_API_KEY"],
        },
        ProviderApi::Single(Arc::new(anthropic_messages_api())),
    )
}
