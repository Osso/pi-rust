//! Port of `packages/ai/src/providers/radius-config.ts`.

use std::sync::Arc;

use pi_js_compat::{
    js_format,
    js_json::{JsString, JsValue as Value},
};
use serde::{Deserialize, Serialize};
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
    pub base_url: JsString,
    pub models: Vec<RadiusGatewayModel>,
}

/// Shallow model shape check from `isRadiusGatewayModel`.
fn is_radius_gateway_model(value: &Value) -> bool {
    let Some(model) = value.as_object() else {
        return false;
    };
    model.get("id").is_some_and(Value::is_string)
        && model.get("name").is_some_and(Value::is_string)
        && model.get("reasoning").and_then(Value::as_bool).is_some()
        && model.get("input").is_some_and(Value::is_array)
        && model.get("cost").is_some_and(|cost| cost.is_object())
        && model.get("contextWindow").and_then(Value::as_f64).is_some()
        && model.get("maxTokens").and_then(Value::as_f64).is_some()
}

/// Typed form of `sanitizeRadiusGatewayConfig`.
///
/// Models that pass the source's shallow check but cannot be represented by the
/// typed [`RadiusGatewayModel`] contract (for example a non-numeric `cost`
/// field) are excluded instead of producing an invalid Rust model.
pub fn sanitize_radius_gateway_config(config: &Value) -> Result<Option<RadiusGatewayConfig>, CallbackError> {
    let Some(object) = config.as_object() else {
        return Ok(None);
    };
    let Some(base_url) = object.get("baseUrl").and_then(Value::as_string) else {
        return Ok(None);
    };
    let Some(models) = object.get("models").and_then(Value::as_array) else {
        return Ok(None);
    };
    let models = models
        .iter()
        .filter(|model| is_radius_gateway_model(model))
        .map(|model| {
            pi_js_compat::js_json::from_value(model)
                .map_err(|error| CallbackError::new(format!("Invalid Radius gateway model: {error}")))
        })
        .collect::<Result<_, _>>()?;
    Ok(Some(RadiusGatewayConfig {
        base_url: base_url.clone(),
        models,
    }))
}

/// Prepends `https://` when missing and strips trailing slashes.
pub fn normalize_radius_gateway_url(value: &JsString) -> JsString {
    let lower = value.to_lowercase();
    let mut normalized = if lower.starts_with("http://") || lower.starts_with("https://") {
        value.clone()
    } else {
        JsString::from("https://") + value
    };
    while normalized.ends_with('/') {
        normalized = normalized.slice(0, normalized.len() - 1);
    }
    normalized
}

/// Reads and sanitizes `credential.gatewayConfig` without scalar JSON projection.
pub fn get_radius_credential_config(
    credential: Option<&OAuthCredential>,
) -> Result<Option<RadiusGatewayConfig>, CallbackError> {
    let Some(config) = credential.and_then(|credential| credential.extra.get("gatewayConfig")) else {
        return Ok(None);
    };
    sanitize_radius_gateway_config(config)
}

/// Converts gateway model entries into full `pi-messages` models.
pub fn get_radius_models_from_config(provider_id: &str, config: &RadiusGatewayConfig) -> Vec<Model> {
    config
        .models
        .iter()
        .map(|model| Model {
            properties: Default::default(),
            id: model.id.clone(),
            name: model.name.clone().into(),
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

pub fn get_radius_models(provider_id: &str, credential: Option<&OAuthCredential>) -> Result<Vec<Model>, CallbackError> {
    Ok(match get_radius_credential_config(credential)? {
        Some(config) => get_radius_models_from_config(provider_id, &config),
        None => Vec::new(),
    })
}

/// Loads `/v1/config` from a Radius gateway.
pub async fn load_radius_gateway_config(
    fetch: &Arc<dyn Fetch>,
    gateway: &JsString,
    api_key: Option<&JsString>,
    signal: Option<CancellationToken>,
) -> Result<RadiusGatewayConfig, CallbackError> {
    let mut headers = vec![("accept".to_owned(), "application/json".to_owned())];
    if let Some(api_key) = api_key {
        headers.push((
            "authorization".to_owned(),
            crate::request::header_value(&js_format!("Bearer {}", api_key))?,
        ));
    }
    let request = HttpRequest {
        method: "GET".to_owned(),
        url: crate::request::request_url(&js_format!("{}/v1/config", gateway))?,
        headers,
        body: None,
    };
    let response = crate::auth::oauth::support::send(fetch, request, signal).await?;
    if !response.ok() {
        return Err(CallbackError::new(js_format!(
            "Could not load Radius config from {}: {}: {}",
            gateway,
            u32::from(response.status),
            truncate_http_body(&response.body)
        )));
    }
    let config = pi_js_compat::js_json::parse_lossless(&response.body)
        .map_err(|error| CallbackError::new(error.into_message()))?;
    sanitize_radius_gateway_config(&config)?
        .ok_or_else(|| CallbackError::new(js_format!("Invalid Radius config from {}", gateway)))
}

/// `body.trim().slice(0, 512)` plus an ellipsis, counted in UTF-16 code units.
fn truncate_http_body(body: &str) -> JsString {
    let trimmed = JsString::from(body).trim();
    if trimmed.len() <= HTTP_BODY_LIMIT {
        trimmed
    } else {
        trimmed.slice(0, HTTP_BODY_LIMIT) + "…"
    }
}
