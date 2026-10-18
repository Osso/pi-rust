//! Port of `packages/ai/src/providers/minimax-cn.ts`.

use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `minimax_cn_provider()`.
pub fn minimax_cn_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "minimax-cn",
            name: "MiniMax CN",
            base_url: Some("https://api.minimaxi.com/anthropic"),
            auth_name: "MiniMax CN API key",
            env_vars: &["MINIMAX_CN_API_KEY"],
        },
        ProviderApi::Single(Arc::new(anthropic_messages_api())),
    )
}
