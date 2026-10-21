//! Provider implementations.

use std::sync::Arc;

use crate::auth::{ProviderAuth, env_api_key_auth};
use crate::models::{CreateProviderOptions, Provider, ProviderApi, create_provider};

pub mod all;
pub mod amazon_bedrock;
pub mod ant_ling;
pub mod anthropic;
pub mod cerebras;
pub mod cloudflare_ai_gateway;
pub mod cloudflare_auth;
pub mod cloudflare_stream;
pub mod cloudflare_workers_ai;
pub mod deepseek;
pub mod faux;
pub mod fireworks;
pub mod github_copilot;
pub mod google;
pub mod google_vertex;
pub mod groq;
pub mod huggingface;
pub mod images;
pub mod kimi_coding;
pub mod minimax;
pub mod minimax_cn;
pub mod mistral;
pub mod models_data;
pub mod moonshotai;
pub mod moonshotai_cn;
pub mod nvidia;
pub mod openai;
pub mod openai_codex;
pub mod opencode;
pub mod opencode_go;
pub mod openrouter;
pub mod openrouter_images;
pub mod radius_config;
pub mod together;
pub mod vercel_ai_gateway;
pub mod xai;
pub mod xiaomi;
pub mod xiaomi_token_plan_ams;
pub mod xiaomi_token_plan_cn;
pub mod xiaomi_token_plan_sgp;
pub mod zai;
pub mod zai_coding_cn;

/// Metadata shared by the plain `envApiKeyAuth` provider factories.
pub(crate) struct ProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: Option<&'static str>,
    pub auth_name: &'static str,
    pub env_vars: &'static [&'static str],
}

/// Builds a provider with the generated catalog and standard api-key auth.
pub(crate) fn env_key_provider(spec: ProviderSpec, api: ProviderApi) -> Arc<dyn Provider> {
    create_provider(CreateProviderOptions {
        id: spec.id.to_owned(),
        name: Some(spec.name.to_owned()),
        base_url: spec.base_url.map(str::to_owned),
        headers: None,
        auth: ProviderAuth::api_key(Arc::new(env_api_key_auth(spec.auth_name, spec.env_vars))),
        models: provider_models(spec.id),
        fetch_models: None,
        filter_models: None,
        api,
    })
}

pub(crate) fn provider_models(provider: &str) -> Vec<crate::types::Model> {
    models_data::provider_models(provider).unwrap_or_else(|| panic!("missing generated catalog for {provider}"))
}

pub(crate) fn single_api(api: crate::api::lazy::LazyApi) -> ProviderApi {
    ProviderApi::Single(Arc::new(api))
}

/// Metadata for factories that supply their own auth (and optional filter).
pub(crate) struct CustomProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: Option<&'static str>,
    pub auth: ProviderAuth,
    pub filter_models: Option<crate::models::FilterModelsFn>,
}

/// Builds a provider with the generated catalog and caller-supplied auth/api.
pub(crate) fn custom_provider(spec: CustomProviderSpec, api: ProviderApi) -> Arc<dyn Provider> {
    create_provider(CreateProviderOptions {
        id: spec.id.to_owned(),
        name: Some(spec.name.to_owned()),
        base_url: spec.base_url.map(str::to_owned),
        headers: None,
        auth: spec.auth,
        models: provider_models(spec.id),
        fetch_models: None,
        filter_models: spec.filter_models,
        api,
    })
}

/// Loader for a bundled OAuth implementation (`lazyOAuth({ load })`).
pub(crate) fn oauth_loader(load: fn() -> Arc<dyn crate::auth::OAuthAuth>) -> crate::auth::OAuthLoader {
    Arc::new(move || Box::pin(async move { Ok(load()) }))
}
