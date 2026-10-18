//! Port of `packages/ai/src/providers/minimax.ts`.

use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `minimax_provider()`.
pub fn minimax_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "minimax",
            name: "MiniMax",
            base_url: Some("https://api.minimax.io/anthropic"),
            auth_name: "MiniMax API key",
            env_vars: &["MINIMAX_API_KEY"],
        },
        ProviderApi::Single(Arc::new(anthropic_messages_api())),
    )
}
