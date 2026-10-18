//! Port of `packages/coding-agent/src/core/provider-attribution.ts`.

use pi_ai::types::{Model, ProviderHeaders};
use url::Url;

use crate::core::settings_manager::SettingsManager;
use crate::core::telemetry::is_install_telemetry_enabled;

const OPENROUTER_HOST: &str = "openrouter.ai";
const NVIDIA_NIM_HOST: &str = "integrate.api.nvidia.com";
const CLOUDFLARE_API_HOST: &str = "api.cloudflare.com";
const CLOUDFLARE_AI_GATEWAY_HOST: &str = "gateway.ai.cloudflare.com";
const OPENCODE_HOST: &str = "opencode.ai";

fn matches_host(base_url: &str, expected_host: &str) -> bool {
    Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == expected_host)
}

fn is_openrouter_model(model: &Model) -> bool {
    model.provider == "openrouter" || model.base_url.contains(OPENROUTER_HOST)
}

fn is_nvidia_nim_model(model: &Model) -> bool {
    model.provider == "nvidia" || matches_host(&model.base_url, NVIDIA_NIM_HOST)
}

fn is_cloudflare_model(model: &Model) -> bool {
    model.provider == "cloudflare-workers-ai"
        || model.provider == "cloudflare-ai-gateway"
        || matches_host(&model.base_url, CLOUDFLARE_API_HOST)
        || matches_host(&model.base_url, CLOUDFLARE_AI_GATEWAY_HOST)
}

fn get_default_attribution_headers(model: &Model, settings_manager: &SettingsManager) -> Option<ProviderHeaders> {
    if !is_install_telemetry_enabled(settings_manager, None) {
        return None;
    }
    let headers: &[(&str, &str)] = if is_openrouter_model(model) {
        &[
            ("HTTP-Referer", "https://pi.dev"),
            ("X-OpenRouter-Title", "pi"),
            ("X-OpenRouter-Categories", "cli-agent"),
        ]
    } else if is_nvidia_nim_model(model) {
        &[("X-BILLING-INVOKE-ORIGIN", "Pi")]
    } else if is_cloudflare_model(model) {
        &[("User-Agent", "pi-coding-agent")]
    } else {
        return None;
    };
    Some(
        headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), Some((*value).to_owned())))
            .collect(),
    )
}

fn get_session_headers(model: &Model, session_id: Option<&str>) -> Option<ProviderHeaders> {
    let session_id = session_id?;
    if model.provider != "opencode" && model.provider != "opencode-go" && !matches_host(&model.base_url, OPENCODE_HOST)
    {
        return None;
    }
    Some(
        [
            ("x-opencode-session".to_owned(), Some(session_id.to_owned())),
            ("x-opencode-client".to_owned(), Some("pi".to_owned())),
        ]
        .into_iter()
        .collect(),
    )
}

/// Merges attribution/session defaults with provider and request headers.
///
/// Later sources overwrite earlier ones exactly like `Object.assign` (case-sensitive).
pub fn merge_provider_attribution_headers(
    model: &Model,
    settings_manager: &SettingsManager,
    session_id: Option<&str>,
    header_sources: &[Option<ProviderHeaders>],
) -> Option<ProviderHeaders> {
    let mut merged: ProviderHeaders = ProviderHeaders::new();
    if let Some(headers) = get_session_headers(model, session_id) {
        merged.extend(headers);
    }
    if let Some(headers) = get_default_attribution_headers(model, settings_manager) {
        merged.extend(headers);
    }
    for headers in header_sources.iter().flatten() {
        merged.extend(headers.clone());
    }
    (!merged.is_empty()).then_some(merged)
}
