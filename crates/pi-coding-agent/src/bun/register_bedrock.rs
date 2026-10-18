//! `bun/register-bedrock.ts`.

use std::sync::Arc;

use pi_ai::bedrock_provider::{bedrock_provider_module, set_bedrock_provider_module};

/// `setBedrockProviderModule(bedrockProviderModule)`.
pub fn register_bedrock() {
    set_bedrock_provider_module(Arc::new(bedrock_provider_module()));
}
