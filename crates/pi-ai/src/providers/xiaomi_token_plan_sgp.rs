//! Port of `packages/ai/src/providers/xiaomi-token-plan-sgp.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `xiaomi_token_plan_sgp_provider()`.
pub fn xiaomi_token_plan_sgp_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "xiaomi-token-plan-sgp",
            name: "Xiaomi Token Plan SGP",
            base_url: Some("https://token-plan-sgp.xiaomimimo.com/v1"),
            auth_name: "Xiaomi Token Plan SGP API key",
            env_vars: &["XIAOMI_TOKEN_PLAN_SGP_API_KEY"],
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
