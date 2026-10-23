//! Port of `packages/coding-agent/src/core/auth-guidance.ts`.

use crate::config::{ConfigError, get_docs_path};
use crate::utils::paths::node_join;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

const UNKNOWN_PROVIDER: &str = "unknown";

fn docs_file(name: &str) -> Result<JsString, ConfigError> {
    Ok(node_join(get_docs_path()?, name))
}

/// Login help pointing at the packaged provider documentation.
pub fn get_provider_login_help() -> Result<JsString, ConfigError> {
    Ok(js_format!(
        "Use /login to log into a provider via OAuth or API key. See:\n  {}\n  {}",
        docs_file("providers.md")?,
        docs_file("models.md")?
    ))
}

pub fn format_no_models_available_message() -> Result<JsString, ConfigError> {
    Ok(js_format!("No models available. {}", get_provider_login_help()?))
}

pub fn format_no_model_selected_message() -> Result<JsString, ConfigError> {
    Ok(js_format!(
        "No model selected.\n\n{}\n\nThen use /model to select a model.",
        get_provider_login_help()?
    ))
}

pub fn format_no_api_key_found_message(provider: &str) -> Result<JsString, ConfigError> {
    let provider_display = if provider == UNKNOWN_PROVIDER {
        "the selected model"
    } else {
        provider
    };
    Ok(js_format!(
        "No API key found for {}.\n\n{}",
        provider_display,
        get_provider_login_help()?
    ))
}
