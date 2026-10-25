//! Port of `packages/ai/src/compat/extension-oauth-types.ts`.

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::JsString;

use crate::request::{BoxFuture, CallbackError};

/// Legacy extension OAuth prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthPrompt {
    pub message: JsString,
    pub placeholder: Option<JsString>,
    pub allow_empty: Option<bool>,
}

/// Legacy extension OAuth authorization link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthAuthInfo {
    pub url: JsString,
    pub instructions: Option<JsString>,
}

/// Legacy extension OAuth device-code notification.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthDeviceCodeInfo {
    pub user_code: JsString,
    pub verification_uri: JsString,
    pub interval_seconds: Option<f64>,
    pub expires_in_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSelectOption {
    pub id: JsString,
    pub label: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthSelectPrompt {
    pub message: JsString,
    pub options: Vec<OAuthSelectOption>,
}

/// Callback surface retained only for coding-agent extension compatibility.
pub trait OAuthLoginCallbacks: Send + Sync {
    fn on_auth(&self, info: OAuthAuthInfo) -> Result<(), CallbackError>;
    fn on_device_code(&self, info: OAuthDeviceCodeInfo) -> Result<(), CallbackError>;
    fn on_prompt(&self, prompt: OAuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>>;
    fn on_progress(&self, _message: &JsString) -> Result<(), CallbackError> {
        Ok(())
    }
    fn on_manual_code_input(&self) -> Option<BoxFuture<'_, Result<JsString, CallbackError>>> {
        None
    }
    fn on_select(&self, prompt: OAuthSelectPrompt) -> BoxFuture<'_, Result<Option<JsString>, CallbackError>>;
    fn signal(&self) -> Option<AbortSignal> {
        None
    }
}

/// `OAuthCredentials` is the canonical OAuth credential shape.
pub type OAuthCredentials = crate::auth::OAuthCredential;
