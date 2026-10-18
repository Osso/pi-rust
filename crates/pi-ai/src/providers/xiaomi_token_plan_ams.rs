//! Port of `packages/ai/src/providers/xiaomi-token-plan-ams.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `xiaomi_token_plan_ams_provider()`.
pub fn xiaomi_token_plan_ams_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "xiaomi-token-plan-ams",
            name: "Xiaomi Token Plan AMS",
            base_url: Some("https://token-plan-ams.xiaomimimo.com/v1"),
            auth_name: "Xiaomi Token Plan AMS API key",
            env_vars: &["XIAOMI_TOKEN_PLAN_AMS_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
