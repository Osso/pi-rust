//! Rust port of Pi's LLM provider abstraction.

pub mod api;
pub mod auth;
pub mod base;
pub mod bedrock_provider;
pub mod compat;
pub mod env_api_keys;
pub mod http;
pub mod image_catalog_generator;
pub mod image_models;
pub mod image_models_generated;
pub mod images;
pub mod images_api_registry;
pub mod images_models;
pub mod legacy_api_aliases;
pub mod model_catalog_generator;
pub mod models;
pub mod oauth;
pub mod providers;
pub mod request;
pub mod session_resources;
pub mod types;
pub mod utils;

// Package-root re-exports mirroring `packages/ai/src/index.ts` (see
// `docs/porting/traceability/ai-index.md` for the symbol mapping).

pub use crate::api::anthropic_messages::{AnthropicEffort, AnthropicOptions, AnthropicThinkingDisplay};
pub use crate::api::azure_openai_responses::AzureOpenAIResponsesOptions;
pub use crate::api::bedrock_converse_stream::{BedrockOptions, BedrockThinkingDisplay};
pub use crate::api::google_generative_ai::GoogleOptions;
pub use crate::api::google_shared::GoogleThinkingLevel;
pub use crate::api::google_vertex::GoogleVertexOptions;
pub use crate::api::lazy::*;
pub use crate::api::mistral_conversations::MistralOptions;
pub use crate::api::openai_codex_responses::{OpenAICodexResponsesOptions, OpenAICodexWebSocketDebugStats};
pub use crate::api::openai_completions::OpenAICompletionsOptions;
pub use crate::api::openai_responses::OpenAIResponsesOptions;
pub use crate::auth::context::*;
pub use crate::auth::credential_store::*;
pub use crate::auth::helpers::*;
pub use crate::auth::types::*;
pub use crate::compat::extension_oauth_types::{
    OAuthAuthInfo, OAuthCredentials, OAuthDeviceCodeInfo, OAuthLoginCallbacks, OAuthPrompt, OAuthSelectOption,
    OAuthSelectPrompt,
};
pub use crate::images_models::*;
pub use crate::models::*;
pub use crate::oauth::{OAuthProvider, OAuthProviderId, OAuthProviderInfo, OAuthProviderInterface};
pub use crate::providers::faux::*;
pub use crate::request::*;
pub use crate::session_resources::*;
pub use crate::types::*;
pub use crate::utils::diagnostics::*;
pub use crate::utils::event_stream::*;
pub use crate::utils::json_parse::*;
pub use crate::utils::overflow::*;
pub use crate::utils::retry::*;
pub use crate::utils::typebox_helpers::*;
pub use crate::utils::validation::*;
