//! Frozen `packages/ai/src/oauth.ts` / `utils/oauth/index.ts` public entry point.

pub use crate::compat::extension_oauth_types::{
    OAuthAuthInfo, OAuthCredentials, OAuthDeviceCodeInfo, OAuthLoginCallbacks, OAuthPrompt, OAuthSelectOption,
    OAuthSelectPrompt,
};

#[path = "oauth_registry.rs"]
mod registry;
pub use registry::*;
