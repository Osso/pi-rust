//! Extension callback failures shared by tool and session layers.

use pi_js_compat::js_json::JsString;
use serde::{Deserialize, Serialize};

/// A handler failure. The source records `Error.message` and `Error.stack`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandlerError {
    pub message: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    #[serde(skip)]
    original: Option<Box<HandlerErrorSource>>,
}
#[derive(Debug, Clone, PartialEq)]
enum HandlerErrorSource {
    Native(pi_js_compat::node_error::NativeNodeError),
    Callback(pi_ai::request::CallbackError),
}
impl HandlerError {
    /// Frozen catch policy: Error.message, otherwise String(rejection).
    pub fn caught_message(&self) -> JsString {
        pi_ai::request::CallbackError::from(self.clone())
            .error_or_string_message()
            .unwrap_or_else(|error| pi_js_compat::callback_error::throw_callback_uncaught(error))
    }

    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            stack: None,
            original: None,
        }
    }
    pub fn with_stack(message: impl Into<JsString>, stack: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            stack: Some(stack.into()),
            original: None,
        }
    }
}
impl std::fmt::Display for HandlerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}
impl std::error::Error for HandlerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self.original.as_deref() {
            Some(HandlerErrorSource::Native(error)) => Some(error),
            Some(HandlerErrorSource::Callback(error)) => Some(error),
            None => None,
        }
    }
}
impl From<pi_js_compat::node_error::NativeNodeError> for HandlerError {
    fn from(error: pi_js_compat::node_error::NativeNodeError) -> Self {
        Self::from(Box::new(error))
    }
}
impl From<Box<pi_js_compat::node_error::NativeNodeError>> for HandlerError {
    fn from(error: Box<pi_js_compat::node_error::NativeNodeError>) -> Self {
        Self {
            message: error.message.clone(),
            stack: None,
            original: Some(Box::new(HandlerErrorSource::Native(*error))),
        }
    }
}
impl From<HandlerError> for pi_ai::request::CallbackError {
    fn from(error: HandlerError) -> Self {
        Self::from_error(error.message.clone(), error)
    }
}
impl From<pi_ai::request::CallbackError> for HandlerError {
    fn from(error: pi_ai::request::CallbackError) -> Self {
        Self {
            message: error.message.clone(),
            stack: None,
            original: Some(Box::new(HandlerErrorSource::Callback(error))),
        }
    }
}
impl From<crate::config::ConfigError> for HandlerError {
    fn from(error: crate::config::ConfigError) -> Self {
        error.into_native_error().into()
    }
}
impl From<crate::utils::paths::PathError> for HandlerError {
    fn from(error: crate::utils::paths::PathError) -> Self {
        Self::from(crate::config::ConfigError::from(error))
    }
}
impl From<String> for HandlerError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for HandlerError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<JsString> for HandlerError {
    fn from(message: JsString) -> Self {
        Self::new(message)
    }
}

impl From<pi_agent_core::agent::ToolError> for HandlerError {
    fn from(error: pi_agent_core::agent::ToolError) -> Self {
        Self::new(error.message)
    }
}
