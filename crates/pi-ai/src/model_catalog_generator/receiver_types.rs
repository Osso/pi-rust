//! Checks at actual source receiver/conversion use points, never a pre-fetch schema pass.
use super::*;

pub(super) fn optional_object(value: &JsValue, label: &str) -> Result<(), GeneratorError> {
    if matches!(value, JsValue::Null) {
        return Ok(());
    }
    emission_types::expect(value, label, "object")
}
pub(super) fn model_entries(provider: &JsValue) -> Result<Vec<(&JsString, &JsValue)>, GeneratorError> {
    optional_object(provider, "models.dev provider")?;
    let models = field(provider, "models");
    // Retain the source outer missing/falsy-provider-data handling before traversal.
    if !is_truthy(models) {
        return Ok(vec![]);
    }
    emission_types::expect(models, "models.dev provider.models", "object")?;
    Ok(entries(models))
}
pub(super) fn model(value: &JsValue) -> Result<(), GeneratorError> {
    emission_types::expect(value, "models.dev model", "object")
}
pub(super) fn optional_field(value: &JsValue, key: &str, label: &str, kind: &str) -> Result<(), GeneratorError> {
    let value = field(value, key);
    if matches!(value, JsValue::Null) {
        return Ok(());
    }
    emission_types::expect(value, label, kind)
}
pub(super) fn tool_call(value: &JsValue) -> Result<bool, GeneratorError> {
    model(value)?;
    optional_field(value, "tool_call", "models.dev tool_call", "boolean")?;
    Ok(field(value, "tool_call") == &JsValue::Bool(true))
}
pub(super) fn deprecated(value: &JsValue) -> Result<bool, GeneratorError> {
    optional_field(value, "status", "models.dev status", "string")?;
    Ok(equal(field(value, "status"), "deprecated"))
}
pub(super) fn reasoning(value: &JsValue, provider: &str, id: &JsString) -> Result<bool, GeneratorError> {
    let forced = match provider {
        "kimi-coding" => id == &JsString::from("k3"),
        "moonshotai" | "moonshotai-cn" => id == &JsString::from("kimi-k3"),
        _ => false,
    };
    if forced {
        return Ok(true);
    }
    optional_field(value, "reasoning", "models.dev reasoning", "boolean")?;
    Ok(field(value, "reasoning") == &JsValue::Bool(true))
}
pub(super) fn provider_npm(value: &JsValue) -> Result<&JsValue, GeneratorError> {
    let provider = field(value, "provider");
    optional_object(provider, "models.dev model.provider")?;
    optional_field(provider, "npm", "models.dev provider.npm", "string")?;
    Ok(field(provider, "npm"))
}
fn string_entries(value: &JsValue, label: &str) -> Result<(), GeneratorError> {
    if matches!(value, JsValue::Array(values) if values.iter().any(|value| !matches!(value, JsValue::String(_)))) {
        return Err(GeneratorError::type_error(&format!("{label} entries must be strings")));
    }
    Ok(())
}
pub(super) fn router_consumed_fields(model: &JsValue, tags: &JsValue) -> Result<(), GeneratorError> {
    string_entries(tags, "OpenRouter supported_parameters")?;
    let architecture = field(model, "architecture");
    optional_object(architecture, "OpenRouter architecture")?;
    string_entries(field(architecture, "modality"), "OpenRouter modality")?;
    optional_object(field(model, "pricing"), "OpenRouter pricing")?;
    optional_object(field(model, "top_provider"), "OpenRouter top_provider")
}
pub(super) fn modalities(value: &JsValue, key: &str) -> Result<(), GeneratorError> {
    let modalities = field(value, "modalities");
    optional_object(modalities, "models.dev modalities")?;
    let input = field(modalities, key);
    if matches!(input, JsValue::Null) {
        return Ok(());
    }
    if matches!(input,JsValue::Array(values) if values.iter().all(|value|matches!(value,JsValue::String(_)))) {
        Ok(())
    } else {
        Err(GeneratorError::type_error(&format!(
            "models.dev modalities.{key} must be an array of strings"
        )))
    }
}
pub(super) fn option_kind(value: &JsValue) -> Result<&JsValue, GeneratorError> {
    if !matches!(value, JsValue::Object(_)) {
        return Err(GeneratorError::type_error(
            "models.dev reasoning options must contain objects",
        ));
    }
    let kind = field(value, "type");
    if !matches!(kind, JsValue::Null | JsValue::String(_)) {
        return Err(GeneratorError::type_error(
            "models.dev reasoning option.type must be a string",
        ));
    }
    Ok(kind)
}
pub(super) fn gateway_consumed_fields(model: &JsValue, tags: &JsValue) -> Result<(), GeneratorError> {
    string_entries(tags, "AI Gateway tags")?;
    let pricing = field(model, "pricing");
    optional_object(pricing, "AI Gateway pricing")?;
    for key in ["input", "output", "input_cache_read", "input_cache_write"] {
        if !matches!(
            field(pricing, key),
            JsValue::Null | JsValue::String(_) | JsValue::Number(_)
        ) {
            return Err(GeneratorError::type_error(&format!(
                "AI Gateway pricing.{key} must be a string or number"
            )));
        }
    }
    Ok(())
}
pub(super) fn options(value: &JsValue) -> Result<&[JsValue], GeneratorError> {
    match value {
        JsValue::Null => Ok(&[]),
        JsValue::Array(values) => Ok(values),
        _ => Err(GeneratorError::type_error(
            "models.dev reasoning_options must be an array",
        )),
    }
}
