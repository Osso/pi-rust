use std::sync::Arc;

use pi_ai::auth::OAuthCredential;
use pi_ai::providers::radius_config::{
    DEFAULT_RADIUS_GATEWAY, get_radius_credential_config, get_radius_models, get_radius_models_from_config,
    load_radius_gateway_config, normalize_radius_gateway_url, sanitize_radius_gateway_config,
};
use serde_json::json;

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::recorded_requests;
use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};

fn model(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": format!("Model {id}"),
        "reasoning": true,
        "thinkingLevelMap": {"high": "high", "max": null},
        "input": ["text", "image"],
        "cost": {"input": 1, "output": 2, "cacheRead": 0.5, "cacheWrite": 3},
        "contextWindow": 128000,
        "maxTokens": 8192
    })
}

fn config() -> serde_json::Value {
    json!({
        "baseUrl": "https://radius.example/v1",
        "models": [model("alpha"), model("beta")]
    })
}

#[test]
fn normalizes_gateway_urls_with_scheme_detection_and_trailing_slashes() {
    assert_eq!(DEFAULT_RADIUS_GATEWAY, "https://radius.pi.dev");
    assert_eq!(
        normalize_radius_gateway_url(&pi_js_compat::js_json::JsString::from("radius.pi.dev")),
        "https://radius.pi.dev"
    );
    assert_eq!(
        normalize_radius_gateway_url(&pi_js_compat::js_json::JsString::from("HTTPS://radius.pi.dev///")),
        "HTTPS://radius.pi.dev"
    );
    assert_eq!(
        normalize_radius_gateway_url(&pi_js_compat::js_json::JsString::from("http://localhost:8080/")),
        "http://localhost:8080"
    );
    assert_eq!(
        normalize_radius_gateway_url(&pi_js_compat::js_json::JsString::from("https://")),
        "https:"
    );
}

#[test]
fn sanitizes_valid_configs_and_filters_shallowly_invalid_models() {
    let mut value = config();
    value["models"].as_array_mut().unwrap().push(json!({"id": "bad"}));
    value["models"].as_array_mut().unwrap().push(json!({
        "id": "missing-cost",
        "name": "x",
        "reasoning": false,
        "input": [],
        "contextWindow": 1,
        "maxTokens": 1
    }));

    let parsed = sanitize_radius_gateway_config(&value).expect("config");
    assert_eq!(parsed.base_url, "https://radius.example/v1");
    assert_eq!(parsed.models.len(), 2);
    assert_eq!(parsed.models[0].id, "alpha");
    assert_eq!(parsed.models[0].context_window, 128000.0);
    assert_eq!(parsed.models[1].thinking_level_map.as_ref().unwrap().len(), 2);
}

#[test]
fn rejects_non_object_or_malformed_configs() {
    assert!(sanitize_radius_gateway_config(&json!(null)).is_none());
    assert!(sanitize_radius_gateway_config(&json!([])).is_none());
    assert!(sanitize_radius_gateway_config(&json!({"baseUrl": "x"})).is_none());
    assert!(sanitize_radius_gateway_config(&json!({"models": []})).is_none());
    assert!(sanitize_radius_gateway_config(&json!({"baseUrl": 1, "models": []})).is_none());
}

#[test]
fn reads_the_gateway_config_from_the_credential_extra_fields() {
    let mut credential = OAuthCredential::new("r", "a", 0.0);
    credential.extra.insert("gatewayConfig".into(), config().into());
    let parsed = get_radius_credential_config(Some(&credential)).expect("gateway config");
    assert_eq!(parsed.models.len(), 2);

    let mut invalid = OAuthCredential::new("r", "a", 0.0);
    invalid
        .extra
        .insert("gatewayConfig".into(), json!({"baseUrl": 1}).into());
    assert!(get_radius_credential_config(Some(&invalid)).is_none());
    assert!(get_radius_credential_config(None).is_none());
}

#[test]
fn builds_pi_messages_models_from_gateway_entries() {
    let parsed = sanitize_radius_gateway_config(&config()).unwrap();
    let models = get_radius_models_from_config("radius", &parsed);
    assert_eq!(models.len(), 2);
    let model = &models[0];
    assert_eq!(model.api, "pi-messages");
    assert_eq!(model.provider, "radius");
    assert_eq!(model.base_url, "https://radius.example/v1");
    assert_eq!(model.id, "alpha");
    assert!(model.reasoning);
    assert_eq!(model.cost.rates.input, 1.0);
    assert_eq!(model.max_tokens, 8192.0);
    assert!(model.auto_compaction_threshold.is_none());

    let mut credential = OAuthCredential::new("r", "a", 0.0);
    credential.extra.insert("gatewayConfig".into(), config().into());
    assert_eq!(get_radius_models("radius", Some(&credential)).len(), 2);
    assert!(get_radius_models("radius", None).is_empty());
}

#[tokio::test]
async fn loads_gateway_config_with_bearer_auth_and_truncated_errors() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &config().to_string())]);
    let parsed = load_radius_gateway_config(
        &(fetch.clone() as Arc<dyn pi_ai::request::Fetch>),
        &pi_js_compat::js_json::JsString::from("https://radius.example"),
        Some(&pi_js_compat::js_json::JsString::from("token")),
        None,
    )
    .await
    .expect("config");
    assert_eq!(parsed.models.len(), 2);
    let requests = recorded_requests(&fetch);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].url, "https://radius.example/v1/config");
    assert_eq!(
        requests[0].headers.get("accept").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        requests[0].headers.get("authorization").map(String::as_str),
        Some("Bearer token")
    );

    let long_body = format!("  {}  ", "é".repeat(600));
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(503, &long_body)]);
    let error = load_radius_gateway_config(
        &(fetch as Arc<dyn pi_ai::request::Fetch>),
        &pi_js_compat::js_json::JsString::from("https://radius.example"),
        None,
        None,
    )
    .await
    .expect_err("error");
    let expected_prefix = format!(
        "Could not load Radius config from https://radius.example: 503: {}",
        "é".repeat(512)
    );
    assert_eq!(error.message, format!("{expected_prefix}…"));

    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, "{\"baseUrl\":1}")]);
    let error = load_radius_gateway_config(
        &(fetch as Arc<dyn pi_ai::request::Fetch>),
        &pi_js_compat::js_json::JsString::from("https://radius.example"),
        None,
        None,
    )
    .await
    .expect_err("error");
    assert_eq!(error.message, "Invalid Radius config from https://radius.example");
}
