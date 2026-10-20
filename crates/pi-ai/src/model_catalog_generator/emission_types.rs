//! Approved native field-type checks, at model emission rather than inside fetch catches.
use super::*;
use pi_js_compat::js_json::{JsObject, stringify_lossless};

fn kind(value: &JsValue) -> &'static str {
    match value {
        JsValue::Null => "null",
        JsValue::Bool(_) => "boolean",
        JsValue::Number(_) => "number",
        JsValue::String(_) => "string",
        JsValue::Array(_) => "array",
        JsValue::Object(_) => "object",
    }
}
pub(super) fn expect(value: &JsValue, path: &str, expected: &str) -> Result<(), GeneratorError> {
    if kind(value) == expected {
        Ok(())
    } else {
        Err(GeneratorError::type_error(&format!(
            "{path} must be {} {expected}",
            if matches!(expected, "array" | "object") {
                "an"
            } else {
                "a"
            }
        )))
    }
}
fn properties<'a>(value: &'a JsValue, path: &str) -> Result<&'a JsObject, GeneratorError> {
    expect(value, path, "object")?;
    let JsValue::Object(object) = value else {
        unreachable!("checked object kind");
    };
    Ok(object)
}
fn rates(value: &JsValue, path: &str) -> Result<(), GeneratorError> {
    properties(value, path)?;
    for field_name in ["input", "output", "cacheRead", "cacheWrite"] {
        expect(field(value, field_name), &format!("{path}.{field_name}"), "number")?;
    }
    Ok(())
}
fn string_map(value: &JsValue, path: &str, nullable: bool) -> Result<(), GeneratorError> {
    for (key, value) in object_entries_lossless(properties(value, path)?) {
        if nullable && matches!(value, JsValue::Null) {
            continue;
        }
        let key = stringify_lossless(&JsValue::String(key.clone()));
        expect(value, &format!("{path}[{key}]"), "string")?;
    }
    Ok(())
}
fn model(value: &JsValue, provider: &JsString, id: &JsString) -> Result<(), GeneratorError> {
    let provider = stringify_lossless(&JsValue::String(provider.clone()));
    let id = stringify_lossless(&JsValue::String(id.clone()));
    let path = format!("Generated model {provider}[{id}]");
    let fields = properties(value, &path)?;
    for name in ["id", "name", "api", "provider", "baseUrl"] {
        expect(field(value, name), &format!("{path}.{name}"), "string")?;
    }
    expect(field(value, "reasoning"), &format!("{path}.reasoning"), "boolean")?;
    let input = field(value, "input");
    expect(input, &format!("{path}.input"), "array")?;
    if let JsValue::Array(input) = input {
        for (index, value) in input.iter().enumerate() {
            expect(value, &format!("{path}.input[{index}]"), "string")?;
        }
    }
    for name in ["contextWindow", "maxTokens"] {
        expect(field(value, name), &format!("{path}.{name}"), "number")?;
    }
    let cost = field(value, "cost");
    rates(cost, &format!("{path}.cost"))?;
    if let Some(tiers) = properties(cost, &format!("{path}.cost"))?.get(&JsString::from("tiers")) {
        expect(tiers, &format!("{path}.cost.tiers"), "array")?;
        if let JsValue::Array(tiers) = tiers {
            for (index, tier) in tiers.iter().enumerate() {
                let path = format!("{path}.cost.tiers[{index}]");
                rates(tier, &path)?;
                expect(
                    field(tier, "inputTokensAbove"),
                    &format!("{path}.inputTokensAbove"),
                    "number",
                )?;
            }
        }
    }
    if let Some(headers) = fields.get(&JsString::from("headers")) {
        string_map(headers, &format!("{path}.headers"), false)?;
    }
    if let Some(map) = fields.get(&JsString::from("thinkingLevelMap")) {
        string_map(map, &format!("{path}.thinkingLevelMap"), true)?;
    }
    if let Some(compat) = fields.get(&JsString::from("compat")) {
        expect(compat, &format!("{path}.compat"), "object")?;
    }
    if let Some(threshold) = fields.get(&JsString::from("autoCompactionThreshold")) {
        expect(threshold, &format!("{path}.autoCompactionThreshold"), "number")?;
    }
    Ok(())
}

pub(super) fn public_catalog(catalog: &JsValue) -> Result<(), GeneratorError> {
    for (provider, models) in object_entries_lossless(properties(catalog, "Generated catalog")?) {
        for (id, value) in object_entries_lossless(properties(models, "Generated provider catalog")?) {
            model(value, provider, id)?;
        }
    }
    Ok(())
}
pub(super) fn grouped_catalog(catalog: &JsValue) -> Result<(), GeneratorError> {
    for (provider, groups) in object_entries_lossless(properties(catalog, "Generated catalog")?) {
        for (_, models) in object_entries_lossless(properties(groups, "Generated API groups")?) {
            for (id, value) in object_entries_lossless(properties(models, "Generated API catalog")?) {
                model(value, provider, id)?;
            }
        }
    }
    Ok(())
}
