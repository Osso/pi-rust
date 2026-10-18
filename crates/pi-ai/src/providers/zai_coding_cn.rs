//! Port of `packages/ai/src/providers/zai-coding-cn.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `zai_coding_cn_provider()`.
pub fn zai_coding_cn_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "zai-coding-cn",
            name: "Z.AI Coding CN",
            base_url: Some("https://open.bigmodel.cn/api/coding/paas/v4"),
            auth_name: "Z.AI Coding CN API key",
            env_vars: &["ZAI_CODING_CN_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
