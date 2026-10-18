//! Port of `packages/ai/src/providers/radius-config.ts`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::auth::OAuthCredential;
use crate::request::{CallbackError, Fetch, HttpRequest};
use crate::types::{Modality, Model, ModelCost, ThinkingLevelMap};

pub const DEFAULT_RADIUS_GATEWAY: &str = "https://radius.pi.dev";
const PI_MESSAGES_API: &str = "pi-messages";
const HTTP_BODY_LIMIT: usize = 512;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusGatewayModel {
    pub id: String,
    pub name: String,
    pub reasoning: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level_map: Option<ThinkingLevelMap>,
    pub input: Vec<Modality>,
    pub cost: ModelCost,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub context_window: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub max_tokens: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusGatewayConfig {
    pub base_url: String,
    pub models: Vec<RadiusGatewayModel>,
}

/// Shallow model shape check from `isRadiusGatewayModel`.
fn is_radius_gateway_model(value: &Value) -> bool {
    let Some(model) = value.as_object() else {
        return false;
    };
    model.get("id").is_some_and(Value::is_string)
        && model.get("name").is_some_and(Value::is_string)
        && model.get("reasoning").is_some_and(Value::is_boolean)
        && model.get("input").is_some_and(Value::is_array)
        && model.get("cost").is_some_and(|cost| cost.is_object())
        && model.get("contextWindow").is_some_and(Value::is_number)
        && model.get("maxTokens").is_some_and(Value::is_number)
}

/// Typed form of `sanitizeRadiusGatewayConfig`.
///
/// Models that pass the source's shallow check but cannot be represented by the
/// typed [`RadiusGatewayModel`] contract (for example a non-numeric `cost`
/// field) are excluded instead of producing an invalid Rust model.
pub fn sanitize_radius_gateway_config(config: &Value) -> Option<RadiusGatewayConfig> {
    let object = config.as_object()?;
    let base_url = object.get("baseUrl")?.as_str()?.to_owned();
    let models = object.get("models")?.as_array()?;
    let models = models
        .iter()
        .filter(|model| is_radius_gateway_model(model))
        .filter_map(|model| serde_json::from_value(model.clone()).ok())
        .collect();
    Some(RadiusGatewayConfig { base_url, models })
}

/// Prepends `https://` when missing and strips trailing slashes.
pub fn normalize_radius_gateway_url(value: &str) -> String {
    let has_scheme = value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || value
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"));
    let with_scheme = if has_scheme {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    with_scheme.trim_end_matches('/').to_owned()
}

/// Reads and sanitizes `credential.gatewayConfig`.
pub fn get_radius_credential_config(credential: Option<&OAuthCredential>) -> Option<RadiusGatewayConfig> {
    sanitize_radius_gateway_config(credential?.extra.get("gatewayConfig")?)
}

/// Converts gateway model entries into full `pi-messages` models.
pub fn get_radius_models_from_config(provider_id: &str, config: &RadiusGatewayConfig) -> Vec<Model> {
    config
        .models
        .iter()
        .map(|model| Model {
            id: model.id.clone(),
            name: model.name.clone(),
            api: PI_MESSAGES_API.to_owned(),
            provider: provider_id.to_owned(),
            base_url: config.base_url.clone(),
            reasoning: model.reasoning,
            thinking_level_map: model.thinking_level_map.clone(),
            input: model.input.clone(),
            cost: model.cost.clone(),
            context_window: model.context_window,
            auto_compaction_threshold: None,
            max_tokens: model.max_tokens,
            sampling_params: None,
            headers: None,
            compat: None,
        })
        .collect()
}

pub fn get_radius_models(provider_id: &str, credential: Option<&OAuthCredential>) -> Vec<Model> {
    match get_radius_credential_config(credential) {
        Some(config) => get_radius_models_from_config(provider_id, &config),
        None => Vec::new(),
    }
}

/// Loads `/v1/config` from a Radius gateway.
pub async fn load_radius_gateway_config(
    fetch: &Arc<dyn Fetch>,
    gateway: &str,
    api_key: Option<&str>,
    signal: Option<CancellationToken>,
) -> Result<RadiusGatewayConfig, CallbackError> {
    let mut headers = vec![("accept".to_owned(), "application/json".to_owned())];
    if let Some(api_key) = api_key {
        headers.push(("authorization".to_owned(), format!("Bearer {api_key}")));
    }
    let request = HttpRequest {
        method: "GET".to_owned(),
        url: format!("{gateway}/v1/config"),
        headers,
        body: None,
    };
    let response = crate::auth::oauth::support::send(fetch, request, signal).await?;
    if !response.ok() {
        return Err(CallbackError::new(format!(
            "Could not load Radius config from {gateway}: {}: {}",
            response.status,
            truncate_http_body(&response.body)
        )));
    }
    let config: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
    sanitize_radius_gateway_config(&config)
        .ok_or_else(|| CallbackError::new(format!("Invalid Radius config from {gateway}")))
}

/// `body.trim().slice(0, 512)` plus an ellipsis, counted in UTF-16 code units.
fn truncate_http_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.encode_utf16().count() <= HTTP_BODY_LIMIT {
        return trimmed.to_owned();
    }
    let mut units = 0;
    let mut end = 0;
    for (index, character) in trimmed.char_indices() {
        let width = character.len_utf16();
        if units + width > HTTP_BODY_LIMIT {
            break;
        }
        units += width;
        end = index + character.len_utf8();
    }
    format!("{}…", &trimmed[..end])
}
