//! Port of `packages/ai/src/providers/cloudflare-workers-ai.ts`.

use std::sync::Arc;

use crate::api::openai_completions::open_ai_completions_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::cloudflare_auth::workers_ai_auth;
use crate::providers::{CustomProviderSpec, custom_provider};

/// `cloudflareWorkersAIProvider()`.
pub fn cloudflare_workers_ai_provider() -> Arc<dyn Provider> {
    custom_provider(
        CustomProviderSpec {
            id: "cloudflare-workers-ai",
            name: "Cloudflare Workers AI",
            base_url: None,
            auth: workers_ai_auth(),
            filter_models: None,
        },
        ProviderApi::Single(Arc::new(open_ai_completions_api())),
    )
}
