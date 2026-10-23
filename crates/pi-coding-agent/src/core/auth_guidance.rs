//! Port of `packages/coding-agent/src/core/auth-guidance.ts`.

use crate::config::{ConfigError, get_docs_path};

const UNKNOWN_PROVIDER: &str = "unknown";

fn docs_file(name: &str) -> Result<String, ConfigError> {
    Ok(get_docs_path()?.join(name).display().to_string())
}

/// Login help pointing at the packaged provider documentation.
pub fn get_provider_login_help() -> Result<String, ConfigError> {
    Ok([
        "Use /login to log into a provider via OAuth or API key. See:".to_owned(),
        format!("  {}", docs_file("providers.md")?),
        format!("  {}", docs_file("models.md")?),
    ]
    .join("\n"))
}

pub fn format_no_models_available_message() -> Result<String, ConfigError> {
    Ok(format!("No models available. {}", get_provider_login_help()?))
}

pub fn format_no_model_selected_message() -> Result<String, ConfigError> {
    Ok(format!(
        "No model selected.\n\n{}\n\nThen use /model to select a model.",
        get_provider_login_help()?
    ))
}

pub fn format_no_api_key_found_message(provider: &str) -> Result<String, ConfigError> {
    let provider_display = if provider == UNKNOWN_PROVIDER {
        "the selected model"
    } else {
        provider
    };
    Ok(format!(
        "No API key found for {provider_display}.\n\n{}",
        get_provider_login_help()?
    ))
}
