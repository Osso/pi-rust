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
pub(super) fn options(value: &JsValue) -> Result<&[JsValue], GeneratorError> {
    match value {
        JsValue::Null => Ok(&[]),
        JsValue::Array(values) => Ok(values),
        _ => Err(GeneratorError::type_error(
            "models.dev reasoning_options must be an array",
        )),
    }
}
