//! Ports `packages/coding-agent/test/sdk-openrouter-attribution.test.ts` (pure
//! header merging) and telemetry behavior from `core/telemetry.ts`.

use pi_ai::types::{Model, ProviderHeaders};
use pi_coding_agent::core::provider_attribution::merge_provider_attribution_headers;
use pi_coding_agent::core::settings_manager::{InMemorySettingsStorage, SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::core::telemetry::is_install_telemetry_enabled;
use serde_json::json;

fn settings(telemetry_enabled: bool) -> SettingsManager {
    let mut manager = SettingsManager::from_storage(
        Box::new(InMemorySettingsStorage::new()),
        SettingsManagerCreateOptions::default(),
    );
    manager.set_enable_install_telemetry(telemetry_enabled);
    manager
}

fn model(provider: &str, base_url: &str, id: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "api": "openai-completions",
        "provider": provider,
        "baseUrl": base_url,
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 128000,
        "maxTokens": 4096,
    }))
    .unwrap()
}

fn headers(pairs: &[(&str, &str)]) -> ProviderHeaders {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), Some((*value).to_owned())))
        .collect()
}

fn merged(
    model: &Model,
    settings_manager: &SettingsManager,
    session_id: Option<&str>,
    sources: &[Option<ProviderHeaders>],
) -> Option<ProviderHeaders> {
    merge_provider_attribution_headers(model, settings_manager, session_id, sources)
}

#[test]
fn telemetry_env_flags_match_the_source() {
    let manager = settings(false);
    for truthy in ["1", "true", "TRUE", "True", "yes", "YES"] {
        assert!(is_install_telemetry_enabled(&manager, Some(truthy)), "{truthy}");
    }
    for falsy in ["", "0", "false", "no", "2", "on"] {
        assert!(!is_install_telemetry_enabled(&manager, Some(falsy)), "{falsy}");
    }
}

#[test]
fn telemetry_falls_back_to_settings_when_env_is_absent() {
    if std::env::var_os("PI_TELEMETRY").is_some() {
        return;
    }
    assert!(!is_install_telemetry_enabled(&settings(false), None));
    assert!(is_install_telemetry_enabled(&settings(true), None));
}

#[test]
fn adds_default_attribution_headers_for_openrouter_models() {
    let headers = merged(
        &model("openrouter", "https://openrouter.ai/api/v1", "openrouter-model"),
        &settings(true),
        None,
        &[],
    )
    .unwrap();
    assert_eq!(headers.get("HTTP-Referer"), Some(&Some("https://pi.dev".to_owned())));
    assert_eq!(headers.get("X-OpenRouter-Title"), Some(&Some("pi".to_owned())));
    assert_eq!(
        headers.get("X-OpenRouter-Categories"),
        Some(&Some("cli-agent".to_owned()))
    );
}

#[test]
fn telemetry_disabled_suppresses_attribution_headers() {
    let headers = merged(
        &model("openrouter", "https://openrouter.ai/api/v1", "openrouter-model"),
        &settings(false),
        None,
        &[],
    );
    assert_eq!(headers, None);
}

#[test]
fn custom_providers_routed_through_openrouter_get_headers() {
    let headers = merged(
        &model("custom-openrouter", "https://openrouter.ai/api/v1", "custom-model"),
        &settings(true),
        None,
        &[],
    )
    .unwrap();
    assert_eq!(headers.get("HTTP-Referer"), Some(&Some("https://pi.dev".to_owned())));
}

#[test]
fn legacy_substring_matching_is_preserved() {
    let headers = merged(
        &model("custom-openrouter", "not-a-url-openrouter.ai", "custom-model"),
        &settings(true),
        None,
        &[],
    )
    .unwrap();
    assert_eq!(headers.get("HTTP-Referer"), Some(&Some("https://pi.dev".to_owned())));
}

#[test]
fn provider_and_request_headers_override_defaults() {
    let provider_headers = headers(&[
        ("HTTP-Referer", "https://provider.example"),
        ("X-OpenRouter-Categories", "provider-category"),
    ]);
    let request_headers = headers(&[("X-OpenRouter-Title", "request-title")]);
    let merged_headers = merged(
        &model("openrouter", "https://openrouter.ai/api/v1", "openrouter-model"),
        &settings(true),
        None,
        &[Some(provider_headers), Some(request_headers)],
    )
    .unwrap();
    assert_eq!(
        merged_headers.get("HTTP-Referer"),
        Some(&Some("https://provider.example".to_owned()))
    );
    assert_eq!(
        merged_headers.get("X-OpenRouter-Title"),
        Some(&Some("request-title".to_owned()))
    );
    assert_eq!(
        merged_headers.get("X-OpenRouter-Categories"),
        Some(&Some("provider-category".to_owned()))
    );
}

#[test]
fn nvidia_provider_and_host_get_billing_header() {
    for model in [
        model("nvidia", "https://example.test/v1", "nvidia-model"),
        model("custom-nim", "https://integrate.api.nvidia.com/v1", "custom-model"),
    ] {
        let headers = merged(&model, &settings(true), None, &[]).unwrap();
        assert_eq!(headers.get("X-BILLING-INVOKE-ORIGIN"), Some(&Some("Pi".to_owned())));
    }
}

#[test]
fn nvidia_headers_are_suppressed_when_disabled_or_routed_elsewhere() {
    let nvidia = model("nvidia", "https://integrate.api.nvidia.com/v1", "nvidia-model");
    assert_eq!(merged(&nvidia, &settings(false), None, &[]), None);

    let openrouter = model(
        "openrouter",
        "https://openrouter.ai/api/v1",
        "nvidia/nemotron-3-super-120b-a12b",
    );
    let headers = merged(&openrouter, &settings(true), None, &[]).unwrap();
    assert_eq!(headers.get("HTTP-Referer"), Some(&Some("https://pi.dev".to_owned())));
    assert!(!headers.contains_key("X-BILLING-INVOKE-ORIGIN"));

    let gateway = model(
        "vercel-ai-gateway",
        "https://ai-gateway.vercel.sh/v1",
        "nvidia/nemotron-3-super-120b-a12b",
    );
    assert_eq!(merged(&gateway, &settings(true), None, &[]), None);
}

#[test]
fn cloudflare_providers_and_hosts_get_a_user_agent() {
    for model in [
        model(
            "cloudflare-workers-ai",
            "https://api.cloudflare.com/client/v4/accounts/x/ai/run",
            "cf-model",
        ),
        model(
            "cloudflare-ai-gateway",
            "https://gateway.ai.cloudflare.com/v1/account/gateway",
            "cf-model",
        ),
        model("custom", "https://api.cloudflare.com/client/v4", "cf-model"),
    ] {
        let headers = merged(&model, &settings(true), None, &[]).unwrap();
        assert_eq!(headers.get("User-Agent"), Some(&Some("pi-coding-agent".to_owned())));
    }
}

#[test]
fn opencode_session_headers_are_added_and_overridable() {
    let opencode = model("opencode", "https://opencode.ai/zen/v1", "opencode-model");
    let default_headers = merged(&opencode, &settings(true), Some("opencode-session"), &[]).unwrap();
    assert_eq!(
        default_headers.get("x-opencode-session"),
        Some(&Some("opencode-session".to_owned()))
    );
    assert_eq!(default_headers.get("x-opencode-client"), Some(&Some("pi".to_owned())));

    let configured = headers(&[
        ("x-opencode-session", "configured-session"),
        ("x-opencode-client", "configured-client"),
    ]);
    let headers = merged(
        &opencode,
        &settings(true),
        Some("opencode-session"),
        &[Some(configured)],
    )
    .unwrap();
    assert_eq!(
        headers.get("x-opencode-session"),
        Some(&Some("configured-session".to_owned()))
    );
    assert_eq!(
        headers.get("x-opencode-client"),
        Some(&Some("configured-client".to_owned()))
    );
}

#[test]
fn session_headers_are_limited_to_opencode_providers_and_hosts() {
    let anthropic = model("anthropic", "https://api.anthropic.com", "claude");
    assert_eq!(merged(&anthropic, &settings(true), Some("session"), &[]), None);

    let custom = model("custom", "https://opencode.ai/zen/v1", "custom-model");
    let headers = merged(&custom, &settings(true), Some("session"), &[]).unwrap();
    assert_eq!(headers.get("x-opencode-session"), Some(&Some("session".to_owned())));

    let no_session = model("opencode", "https://opencode.ai/zen/v1", "opencode-model");
    assert_eq!(merged(&no_session, &settings(true), None, &[]), None);
}
