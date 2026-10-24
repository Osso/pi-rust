//! Port of `packages/ai/src/providers/openai-codex.ts`.

use std::sync::Arc;

use crate::api::openai_codex_responses::openai_codex_responses_api;
use crate::auth::oauth::load::load_openai_codex_oauth;
use crate::auth::{LazyOAuthInput, ProviderAuth, lazy_oauth};
use crate::models::{CreateProviderOptions, Provider, ProviderApi, create_provider};
use crate::providers::{models_data, oauth_loader};
use crate::types::Model;

const DEFAULT_ID: &str = "openai-codex";
const DEFAULT_NAME: &str = "OpenAI Codex";
const BASE_URL: &str = "https://chatgpt.com/backend-api";
const OAUTH_NAME: &str = "OpenAI (ChatGPT Plus/Pro)";

/// `openaiCodexProvider()`.
pub fn openai_codex_provider() -> Arc<dyn Provider> {
    openai_codex_provider_with_id(DEFAULT_ID, DEFAULT_NAME)
}

/// `openaiCodexProvider(id, name)`.
///
/// TypeScript rewrites every `OPENAI_CODEX_MODELS` entry to `provider: id`; the
/// GlobalComix alias therefore uses the `openai-codex` catalog, not the
/// generated `openai-codex-gc` catalog (see traceability).
pub fn openai_codex_provider_with_id(id: &str, name: &str) -> Arc<dyn Provider> {
    create_provider(CreateProviderOptions {
        id: id.to_owned(),
        name: Some(name.to_owned()),
        base_url: Some(BASE_URL.into()),
        headers: None,
        auth: codex_auth(),
        models: codex_models(id),
        refresh_models: None,
        api: ProviderApi::Single(Arc::new(openai_codex_responses_api())),
    })
}

fn codex_models(provider_id: &str) -> Vec<Model> {
    models_data::provider_models(DEFAULT_ID)
        .unwrap_or_else(|| panic!("missing generated catalog for {DEFAULT_ID}"))
        .into_iter()
        .map(|mut model| {
            model.provider = provider_id.to_owned();
            model
        })
        .collect()
}

fn codex_auth() -> ProviderAuth {
    ProviderAuth {
        api_key: None,
        oauth: Some(Arc::new(lazy_oauth(LazyOAuthInput {
            name: OAUTH_NAME.into(),
            load: oauth_loader(load_openai_codex_oauth),
        }))),
    }
}
