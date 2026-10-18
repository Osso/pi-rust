//! Port of `packages/ai/src/providers/qwen-token-plan-individual.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `qwen_token_plan_individual_provider()`.
pub fn qwen_token_plan_individual_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "qwen-token-plan-individual",
            name: "Qwen Token Plan Individual",
            base_url: Some("https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"),
            auth_name: "Qwen Token Plan Individual API key",
            env_vars: &["QWEN_TOKEN_PLAN_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
