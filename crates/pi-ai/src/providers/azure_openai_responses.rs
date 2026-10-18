//! Port of `packages/ai/src/providers/azure-openai-responses.ts`.

use std::sync::Arc;

use crate::api::azure_openai_responses::azure_open_ai_responses_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::{ProviderSpec, env_key_provider};

/// `azureOpenAIResponsesProvider()`.
pub fn azure_openai_responses_provider() -> Arc<dyn Provider> {
    env_key_provider(
        ProviderSpec {
            id: "azure-openai-responses",
            name: "Azure OpenAI",
            base_url: None,
            auth_name: "Azure OpenAI API key",
            env_vars: &["AZURE_OPENAI_API_KEY"],
        },
        ProviderApi::Single(Arc::new(azure_open_ai_responses_api())),
    )
}
