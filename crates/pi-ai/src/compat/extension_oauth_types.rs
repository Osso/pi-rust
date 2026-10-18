//! Port of `packages/ai/src/compat/extension-oauth-types.ts`.

use tokio_util::sync::CancellationToken;

use crate::request::BoxFuture;

/// Legacy extension OAuth prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthPrompt {
    pub message: String,
    pub placeholder: Option<String>,
    pub allow_empty: Option<bool>,
}

/// Legacy extension OAuth authorization link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthAuthInfo {
    pub url: String,
    pub instructions: Option<String>,
}

/// Legacy extension OAuth device-code notification.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthDeviceCodeInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub interval_seconds: Option<f64>,
    pub expires_in_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSelectOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSelectPrompt {
    pub message: String,
    pub options: Vec<OAuthSelectOption>,
}

/// Callback surface retained only for coding-agent extension compatibility.
pub trait OAuthLoginCallbacks: Send + Sync {
    fn on_auth(&self, info: OAuthAuthInfo);
    fn on_device_code(&self, info: OAuthDeviceCodeInfo);
    fn on_prompt(&self, prompt: OAuthPrompt) -> BoxFuture<'_, String>;
    fn on_progress(&self, _message: &str) {}
    fn on_manual_code_input(&self) -> Option<BoxFuture<'_, String>> {
        None
    }
    fn on_select(&self, prompt: OAuthSelectPrompt) -> BoxFuture<'_, Option<String>>;
    fn signal(&self) -> Option<CancellationToken> {
        None
    }
}

/// `OAuthCredentials` is the canonical OAuth credential shape.
pub type OAuthCredentials = crate::auth::OAuthCredential;
