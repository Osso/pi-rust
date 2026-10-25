//! Session import errors (`core/session-errors.ts`).

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::fmt;

/// Raised when `/import` references a JSONL file path that does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionImportFileNotFoundError {
    pub file_path: JsString,
}

impl SessionImportFileNotFoundError {
    /// The source error's `name`.
    pub const NAME: &'static str = "SessionImportFileNotFoundError";
    /// Source message prefix; identity is retained separately from display text.
    pub const MESSAGE_PREFIX: &'static str = "File not found: ";

    pub fn new(file_path: impl Into<JsString>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }

    pub fn message(&self) -> JsString {
        JsString::from(Self::MESSAGE_PREFIX) + &self.file_path
    }

    pub fn into_callback(self) -> pi_ai::request::CallbackError {
        let native = pi_js_compat::node_error::NativeNodeError::new(Self::NAME, self.message()).with_property(
            "filePath",
            pi_js_compat::js_json::JsValue::String(self.file_path.clone()),
        );
        pi_ai::request::CallbackError::from(native).with_original(std::sync::Arc::new(self))
    }
}

impl fmt::Display for SessionImportFileNotFoundError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message()))
    }
}

impl std::error::Error for SessionImportFileNotFoundError {}
