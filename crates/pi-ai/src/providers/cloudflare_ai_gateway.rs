//! Port of `packages/ai/src/providers/cloudflare-ai-gateway.ts`.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::api::anthropic_messages_lazy::anthropic_messages_api;
use crate::api::openai_completions::open_ai_completions_api;
use crate::api::openai_responses::open_ai_responses_api;
use crate::models::{Provider, ProviderApi};
use crate::providers::cloudflare_auth::ai_gateway_auth;
use crate::providers::cloudflare_stream::cloudflare_streams;
use crate::providers::{CustomProviderSpec, custom_provider};
use crate::request::ProviderStreams;

/// `cloudflareAIGatewayProvider()`.
pub fn cloudflare_ai_gateway_provider() -> Arc<dyn Provider> {
    let mut api: BTreeMap<String, Arc<dyn ProviderStreams>> = BTreeMap::new();
    api.insert(
        "anthropic-messages".to_owned(),
        Arc::new(cloudflare_streams(Arc::new(anthropic_messages_api()))),
    );
    api.insert(
        "openai-completions".to_owned(),
        Arc::new(cloudflare_streams(Arc::new(open_ai_completions_api()))),
    );
    api.insert(
        "openai-responses".to_owned(),
        Arc::new(cloudflare_streams(Arc::new(open_ai_responses_api()))),
    );
    custom_provider(
        CustomProviderSpec {
            id: "cloudflare-ai-gateway",
            name: "Cloudflare AI Gateway",
            base_url: None,
            auth: ai_gateway_auth(),
            filter_models: None,
        },
        ProviderApi::Map(api),
    )
}
