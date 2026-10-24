//! The frozen script's consumed JSON property/method operations, not a schema.
use super::*;

pub(super) fn property(value: Option<&JsValue>, key: &str) -> Result<Option<JsValue>, GeneratorError> {
    match value {
        None => Err(type_error(&format!(
            "Cannot read properties of undefined (reading '{key}')"
        ))),
        Some(JsValue::Null) => Err(type_error(&format!("Cannot read properties of null (reading '{key}')"))),
        Some(JsValue::Object(map)) => Ok(map.get(&JsString::from(key)).cloned()),
        Some(JsValue::Array(items)) if key == "length" => Ok(Some(number(items.len() as u64))),
        Some(JsValue::String(string)) if key == "length" => Ok(Some(number(string.len() as u64))),
        _ => Ok(None),
    }
}
pub(super) fn optional_property(value: Option<&JsValue>, key: &str) -> Result<Option<JsValue>, GeneratorError> {
    if matches!(value, None | Some(JsValue::Null)) {
        Ok(None)
    } else {
        property(value, key)
    }
}
pub(super) fn provider_models(data: &JsValue, provider: &str) -> Result<Option<JsValue>, GeneratorError> {
    let provider = property(Some(data), provider)?;
    optional_property(provider.as_ref(), "models")
}
pub(super) fn tool_models(
    data: &JsValue,
    provider: &str,
) -> Result<impl Iterator<Item = Result<(JsString, JsValue), GeneratorError>>, GeneratorError> {
    let models = provider_models(data, provider)?
        .filter(is_truthy)
        .unwrap_or(JsValue::Null);
    let entries: Vec<_> = match models {
        JsValue::Object(map) => object_entries_lossless(&map)
            .into_iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        JsValue::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(index, value)| (JsString::from(index.to_string()), value))
            .collect(),
        JsValue::String(string) => string
            .units()
            .iter()
            .enumerate()
            .map(|(index, unit)| {
                (
                    JsString::from(index.to_string()),
                    JsValue::String(JsString::from_utf16(vec![*unit])),
                )
            })
            .collect(),
        _ => vec![],
    };
    Ok(entries
        .into_iter()
        .filter_map(|(id, model)| match property(Some(&model), "tool_call") {
            Ok(Some(JsValue::Bool(true))) => Some(Ok((id, model))),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        }))
}
pub(super) fn image_input(source: &JsValue, expression: &str) -> Result<bool, GeneratorError> {
    let modalities = property(Some(source), "modalities")?;
    let input = optional_property(modalities.as_ref(), "input")?;
    optional_includes(input.as_ref(), "image", expression)
}
pub(super) fn optional_includes(
    value: Option<&JsValue>,
    needle: &str,
    expression: &str,
) -> Result<bool, GeneratorError> {
    match value {
        None | Some(JsValue::Null) => Ok(false),
        Some(JsValue::Array(_) | JsValue::String(_)) => Ok(includes(value.expect("present receiver"), needle)),
        _ => Err(type_error(&format!("{expression} is not a function"))),
    }
}
