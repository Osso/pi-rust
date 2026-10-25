//! Theme error values and source message formatting.

use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};

/// Theme loading and color-resolution failures, with TypeScript message text.
#[derive(Clone, Debug, PartialEq)]
pub enum ThemeError {
    Config(Box<crate::config::ConfigError>),
    InvalidHexColor(JsString),
    InvalidColorValue(JsString),
    CircularVariableReference(JsString),
    VariableReferenceNotFound(JsString),
    Parse { label: JsString, message: JsString },
    Syntax { message: JsString },
    Io { message: JsString },
    Invalid { label: JsString, message: JsString },
    NameWithSlash(JsString),
    NotFound(JsString),
    NoSourcePath(JsString),
    NotInitialized,
}

impl ThemeError {
    /// Source error message, before any diagnostic/output encoding.
    pub fn message(&self) -> JsString {
        match self {
            Self::Config(error) => error.message().clone(),
            Self::InvalidHexColor(hex) => js_format!("Invalid hex color: {}", hex),
            Self::InvalidColorValue(color) => js_format!("Invalid color value: {}", color),
            Self::CircularVariableReference(name) => js_format!("Circular variable reference detected: {}", name),
            Self::VariableReferenceNotFound(name) => js_format!("Variable reference not found: {}", name),
            Self::Parse { label, message } => js_format!("Failed to parse theme {}: {}", label, message),
            Self::Syntax { message } | Self::Io { message } | Self::Invalid { message, .. } => message.clone(),
            Self::NameWithSlash(name) => js_format!(
                "Invalid theme name \"{}\": theme names cannot contain \"/\" because it is reserved for automatic light/dark theme settings.",
                name
            ),
            Self::NotFound(name) => js_format!("Theme not found: {}", name),
            Self::NoSourcePath(name) => js_format!("Theme \"{}\" does not have a source path for export", name),
            Self::NotInitialized => "Theme not initialized. Call initTheme() first.".into(),
        }
    }

    /// Node error name: raw `JSON.parse` failures are SyntaxErrors.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Syntax { .. } => "SyntaxError",
            Self::Config(error) => error.label(),
            _ => crate::utils::uncaught::ERROR,
        }
    }
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message()))
    }
}

impl std::error::Error for ThemeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}
impl From<crate::config::ConfigError> for ThemeError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self::Config(Box::new(error))
    }
}
