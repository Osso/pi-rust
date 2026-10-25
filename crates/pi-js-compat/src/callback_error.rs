//! Error values shared across native callback boundaries.

use crate as pi_js_compat;
use crate::js_json::JsValue;
use std::sync::Arc;

/// Error surfaced by a callback or fetch implementation (`unknown` in TypeScript).
#[derive(Debug, Clone)]
pub struct CallbackError {
    pub message: pi_js_compat::js_json::JsString,
    /// Top-level error identity; legacy callbacks default to `Error`.
    pub name: pi_js_compat::js_json::JsString,
    /// Exact rejection when a native boundary throws a non-legacy value.
    pub rejection: Option<Box<pi_js_compat::abort::AbortReason>>,
    /// Node error code, when the boundary supplies one.
    pub code: Option<String>,
    /// Native diagnostics, not a reconstructed JavaScript cause object.
    pub cause: Option<String>,
    /// Original Rust error retained independently from its JavaScript projection.
    pub original: Option<Arc<dyn std::error::Error + Send + Sync>>,
}

/// A callback rejection reaches an uncaught process/callback boundary.
pub fn throw_callback_uncaught(error: CallbackError) -> ! {
    std::panic::panic_any(error)
}

impl PartialEq for CallbackError {
    fn eq(&self, other: &Self) -> bool {
        let same_value = (&self.message, &self.name, &self.rejection, &self.code, &self.cause)
            == (&other.message, &other.name, &other.rejection, &other.code, &other.cause);
        let same_source = match (&self.original, &other.original) {
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        };
        same_value && same_source
    }
}

impl CallbackError {
    pub fn new(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self {
            message: message.into(),
            name: "Error".into(),
            rejection: None,
            code: None,
            cause: None,
            original: None,
        }
    }
    pub fn with_original(mut self, error: Arc<dyn std::error::Error + Send + Sync>) -> Self {
        self.original = Some(error);
        self
    }

    /// Carry a native throw through typed Rust wrappers without replacing its identity.
    pub fn from_error<E: std::error::Error + Send + Sync + 'static>(
        message: pi_js_compat::js_json::JsString,
        error: E,
    ) -> Self {
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        while let Some(current) = source {
            if let Some(callback) = current.downcast_ref::<Self>() {
                return callback.clone().with_original(Arc::new(error));
            }
            if let Some(native) = current.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
                return Self::from_rejection(native.clone().into()).with_original(Arc::new(error));
            }
            source = current.source();
        }
        Self::new(message).with_original(Arc::new(error))
    }
}

impl CallbackError {
    pub fn from_rejection(rejection: pi_js_compat::abort::AbortReason) -> Self {
        let (name, message) = match &rejection {
            pi_js_compat::abort::AbortReason::Error(error) | pi_js_compat::abort::AbortReason::DomException(error) => {
                (error.name.clone(), error.message.clone())
            }
            pi_js_compat::abort::AbortReason::Value(_) => (Default::default(), Default::default()),
        };
        Self {
            name,
            message,
            rejection: Some(Box::new(rejection)),
            code: None,
            cause: None,
            original: None,
        }
    }

    /// Source catch policy: Error.message, otherwise the shared JS String(value) authority.
    pub fn error_or_string_message(&self) -> Result<pi_js_compat::js_json::JsString, Self> {
        match self.thrown_value() {
            pi_js_compat::abort::AbortReason::Error(error) | pi_js_compat::abort::AbortReason::DomException(error) => {
                Ok(error.message)
            }
            pi_js_compat::abort::AbortReason::Value(value) => {
                pi_js_compat::js_json::to_js_string(&value).map_err(|message| {
                    Self::from(pi_js_compat::node_error::NativeNodeError::from_constructor(
                        pi_js_compat::node_error::NativeErrorClass::TypeError,
                        message,
                    ))
                })
            }
        }
    }

    /// Provider catches that use Error.message and otherwise JSON.stringify.
    pub fn error_or_json_message(&self) -> pi_js_compat::js_json::JsString {
        match self.thrown_value() {
            pi_js_compat::abort::AbortReason::Error(error) | pi_js_compat::abort::AbortReason::DomException(error) => {
                error.message
            }
            pi_js_compat::abort::AbortReason::Value(value) => pi_js_compat::js_json::stringify_lossless(&value).into(),
        }
    }

    /// Preserve branding and native properties; legacy callback fields describe an Error.
    pub fn thrown_value(&self) -> pi_js_compat::abort::AbortReason {
        self.rejection.as_deref().cloned().unwrap_or_else(|| {
            let mut error = pi_js_compat::node_error::NativeNodeError::new(self.name.clone(), self.message.clone());
            if let Some(code) = &self.code {
                error = error.with_property("code", JsValue::String(code.as_str().into()));
            }
            if let Some(cause) = &self.cause {
                error = error.with_property("cause", JsValue::String(cause.as_str().into()));
            }
            pi_js_compat::abort::AbortReason::Error(error)
        })
    }
}

impl From<pi_js_compat::node_error::NativeNodeError> for CallbackError {
    fn from(error: pi_js_compat::node_error::NativeNodeError) -> Self {
        Self::from_rejection(error.into())
    }
}

impl From<pi_js_compat::js_json::JsString> for CallbackError {
    fn from(message: pi_js_compat::js_json::JsString) -> Self {
        Self::new(message)
    }
}

impl From<std::io::Error> for CallbackError {
    fn from(error: std::io::Error) -> Self {
        Self::from_error(error.to_string().into(), error)
    }
}

impl From<String> for CallbackError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for CallbackError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl std::fmt::Display for CallbackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}

impl std::error::Error for CallbackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.original
            .as_deref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}
