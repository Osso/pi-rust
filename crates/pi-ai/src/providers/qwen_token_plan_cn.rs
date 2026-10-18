//! Port of `packages/ai/src/providers/qwen-token-plan-cn.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `qwen_token_plan_cn_provider()`.
pub fn qwen_token_plan_cn_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "qwen-token-plan-cn",
            name: "Qwen Token Plan CN",
            base_url: Some("https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"),
            auth_name: "Qwen Token Plan CN API key",
            env_vars: &["QWEN_TOKEN_PLAN_CN_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
