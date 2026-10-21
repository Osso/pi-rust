//! Port of `packages/ai/src/providers/all.ts`.
//!

use std::sync::Arc;

use crate::images_models::{ImagesModelsCollection, ImagesProvider, MutableImagesModels, create_images_models};
use crate::models::{CreateModelsOptions, ModelsCollection, MutableModels, Provider, create_models};
use crate::providers::{
    amazon_bedrock::amazon_bedrock_provider, ant_ling::ant_ling_provider, anthropic::anthropic_provider,
    cerebras::cerebras_provider, cloudflare_ai_gateway::cloudflare_ai_gateway_provider,
    cloudflare_workers_ai::cloudflare_workers_ai_provider, deepseek::deepseek_provider, fireworks::fireworks_provider,
    github_copilot::github_copilot_provider, google::google_provider, google_vertex::google_vertex_provider,
    groq::groq_provider, huggingface::huggingface_provider, kimi_coding::kimi_coding_provider,
    minimax::minimax_provider, minimax_cn::minimax_cn_provider, mistral::mistral_provider, models_data,
    moonshotai::moonshotai_provider, moonshotai_cn::moonshotai_cn_provider, nvidia::nvidia_provider,
    openai::openai_provider, openai_codex::openai_codex_provider, opencode::opencode_provider,
    opencode_go::opencode_go_provider, openrouter::openrouter_provider, together::together_provider,
    vercel_ai_gateway::vercel_ai_gateway_provider, xai::xai_provider, xiaomi::xiaomi_provider,
    xiaomi_token_plan_ams::xiaomi_token_plan_ams_provider, xiaomi_token_plan_cn::xiaomi_token_plan_cn_provider,
    xiaomi_token_plan_sgp::xiaomi_token_plan_sgp_provider, zai::zai_provider, zai_coding_cn::zai_coding_cn_provider,
};
use crate::types::Model;

/// `BuiltinProvider = keyof typeof MODELS`: a generated catalog key.
pub type BuiltinProvider = String;

/// `getBuiltinModel(provider, modelId)`.
pub fn get_builtin_model(provider: &str, model_id: &str) -> Option<Model> {
    models_data::builtin_model(provider, model_id)
}

/// `getBuiltinProviders()`: catalog keys, not provider factories.
pub fn get_builtin_providers() -> Vec<String> {
    models_data::builtin_provider_ids()
}

/// `getBuiltinModels(provider)`.
pub fn get_builtin_models(provider: &str) -> Vec<Model> {
    models_data::provider_models(provider).unwrap_or_default()
}

/// Source-absent staged compilation dependency of coding-agent model_runtime.
/// Not frozen-source parity; remove with that consumer's re-pin.
pub fn get_builtin_model_data_generated_at() -> Option<f64> {
    models_data::model_data_generated_at()
}

/// All built-in providers, freshly constructed.
pub fn builtin_providers() -> Vec<Arc<dyn Provider>> {
    vec![
        amazon_bedrock_provider(),
        ant_ling_provider(),
        anthropic_provider(),
        cerebras_provider(),
        cloudflare_ai_gateway_provider(),
        cloudflare_workers_ai_provider(),
        deepseek_provider(),
        fireworks_provider(),
        github_copilot_provider(),
        google_provider(),
        google_vertex_provider(),
        groq_provider(),
        huggingface_provider(),
        kimi_coding_provider(),
        minimax_provider(),
        minimax_cn_provider(),
        mistral_provider(),
        moonshotai_provider(),
        moonshotai_cn_provider(),
        nvidia_provider(),
        openai_provider(),
        openai_codex_provider(),
        crate::providers::openai_codex::openai_codex_provider_with_id("openai-codex-gc", "OpenAI Codex (GlobalComix)"),
        crate::providers::openai_codex::openai_codex_provider_with_id("openai-codex-team", "OpenAI Codex (Team)"),
        opencode_provider(),
        opencode_go_provider(),
        openrouter_provider(),
        together_provider(),
        vercel_ai_gateway_provider(),
        xai_provider(),
        xiaomi_provider(),
        xiaomi_token_plan_ams_provider(),
        xiaomi_token_plan_cn_provider(),
        xiaomi_token_plan_sgp_provider(),
        zai_provider(),
        zai_coding_cn_provider(),
    ]
}

/// A `Models` collection with every built-in provider registered.
pub fn builtin_models(options: CreateModelsOptions) -> ModelsCollection {
    let models = create_models(options);
    for provider in builtin_providers() {
        models.set_provider(provider);
    }
    models
}

/// All built-in image-generation providers, freshly constructed.
pub fn builtin_images_providers() -> Vec<Arc<dyn ImagesProvider>> {
    vec![crate::providers::openrouter_images::openrouter_images_provider()]
}

/// An `ImagesModels` collection with every built-in image provider registered.
pub fn builtin_images_models(options: CreateModelsOptions) -> ImagesModelsCollection {
    let models = create_images_models(options);
    for provider in builtin_images_providers() {
        models.set_provider(provider);
    }
    models
}
