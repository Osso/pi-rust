//! Port of `packages/ai/src/providers/moonshotai-cn.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `moonshotai_cn_provider()`.
pub fn moonshotai_cn_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "moonshotai-cn",
            name: "Moonshot AI CN",
            base_url: Some("https://api.moonshot.cn/v1"),
            auth_name: "Moonshot AI API key",
            env_vars: &["MOONSHOT_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
