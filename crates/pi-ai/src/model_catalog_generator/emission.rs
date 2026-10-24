//! Materialize the frozen emitter's literals, never evaluate injected expressions.
use super::*;
use pi_js_compat::js_json::{encode_node_utf8, stringify_lossless, to_js_string};

fn emission_error(model: &JsValue, field: &str, reason: &str) -> GeneratorError {
    named_error(
        "Error",
        pi_js_compat::js_format!(
            "Cannot emit provider {} model {} field {}: {}",
            field_value(model, "provider"),
            field_value(model, "id"),
            field,
            reason
        ),
    )
}
fn field_value(model: &JsValue, key: &str) -> JsString {
    JsString::from(stringify_lossless(field(model, key)))
}
fn string_value(model: &JsValue, key: &str, value: &JsValue) -> Result<JsValue, GeneratorError> {
    let source = to_js_string(value).map_err(|reason| emission_error(model, key, reason))?;
    if matches!(key, "id" | "provider" | "api") && source.to_utf8().is_err() {
        return Err(emission_error(
            model,
            key,
            "opaque identifier contains an unpaired UTF-16 unit",
        ));
    }
    // writeFileSync encodes the literal source, not the evaluated string value.
    let encoded = encode_node_utf8(&source);
    let literal = JsString::from(format!("\"{encoded}\""));
    let value = parse_string(&literal).ok_or_else(|| emission_error(model, key, "unparseable string literal"))?;
    if matches!(key, "id" | "provider" | "api") && value.to_utf8().is_err() {
        return Err(emission_error(
            model,
            key,
            "opaque identifier contains an unpaired UTF-16 unit",
        ));
    }
    Ok(JsValue::String(value))
}
fn literal_value(model: &JsValue, key: &str, value: &JsValue) -> Result<JsValue, GeneratorError> {
    let source = to_js_string(value).map_err(|reason| emission_error(model, key, reason))?;
    let source = JsString::from(encode_node_utf8(&source));
    let trimmed = source.trim();
    let literal = match trimmed.to_utf8().as_deref() {
        Ok("true") => Some(JsValue::Bool(true)),
        Ok("false") => Some(JsValue::Bool(false)),
        Ok("null") => Some(JsValue::Null),
        Ok("NaN" | "Infinity" | "+Infinity" | "-Infinity") => {
            return Err(emission_error(
                model,
                key,
                "nonfinite literal has no native artifact encoding",
            ));
        }
        _ => parse_string(&trimmed)
            .map(JsValue::String)
            .or_else(|| parse_numeric_literal(&trimmed).map(JsValue::Number)),
    }
    .ok_or_else(|| emission_error(model, key, "not a supported JavaScript literal expression"))?;
    if matches!(literal,JsValue::Number(value) if !value.is_finite()) {
        return Err(emission_error(
            model,
            key,
            "nonfinite literal has no native artifact encoding",
        ));
    }
    Ok(literal)
}

fn parse_numeric_literal(source: &JsString) -> Option<f64> {
    let source = source.to_utf8().ok()?;
    let unsigned = source.strip_prefix(['+', '-']).unwrap_or(&source);
    if unsigned.is_empty() || unsigned.starts_with(['+', '-']) || unsigned.chars().any(char::is_whitespace) {
        return None;
    }
    let negative = source.starts_with('-');
    let digits = strip_numeric_separators(unsigned)?;
    // Number() accepts legacy leading-zero decimals, unlike strict modules.
    if digits.starts_with('0') && digits.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let value = pi_js_compat::number::parse_number(&digits);
    if value.is_nan() {
        None
    } else {
        Some(if negative { -value } else { value })
    }
}

fn strip_numeric_separators(source: &str) -> Option<String> {
    let radix = match source.get(..2) {
        Some("0x" | "0X") => 16,
        Some("0o" | "0O") => 8,
        Some("0b" | "0B") => 2,
        _ => 10,
    };
    let bytes = source.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'_' {
            continue;
        }
        let before = *bytes.get(index.checked_sub(1)?)?;
        let after = *bytes.get(index + 1)?;
        if !(char::from(before).is_digit(radix) && char::from(after).is_digit(radix)) {
            return None;
        }
    }
    Some(source.replace('_', ""))
}

fn hex_units(units: &[u16]) -> Option<u32> {
    units.iter().try_fold(0u32, |value, unit| {
        let digit = char::from_u32(u32::from(*unit))?.to_digit(16)?;
        value.checked_mul(16)?.checked_add(digit)
    })
}
fn unicode_escape(units: &[u16], position: &mut usize, output: &mut Vec<u16>) -> Option<()> {
    if units.get(*position) == Some(&u16::from(b'{')) {
        *position += 1;
        let start = *position;
        while units.get(*position).is_some_and(|unit| *unit != u16::from(b'}')) {
            *position += 1;
        }
        if start == *position || units.get(*position) != Some(&u16::from(b'}')) {
            return None;
        }
        let scalar = hex_units(&units[start..*position])?;
        if scalar > 0x10ffff {
            return None;
        }
        *position += 1;
        if scalar <= 0xffff {
            output.push(scalar as u16);
        } else {
            let scalar = scalar - 0x10000;
            output.extend([0xd800 + (scalar >> 10) as u16, 0xdc00 + (scalar & 0x3ff) as u16]);
        }
    } else {
        let end = position.checked_add(4)?;
        output.push(hex_units(units.get(*position..end)?)? as u16);
        *position = end;
    }
    Some(())
}
fn append_escape(units: &[u16], position: &mut usize, output: &mut Vec<u16>) -> Option<()> {
    let escaped = *units.get(*position)?;
    *position += 1;
    match escaped {
        0x62 => output.push(8),
        0x66 => output.push(12),
        0x6e => output.push(10),
        0x72 => output.push(13),
        0x74 => output.push(9),
        0x76 => output.push(11),
        0x30 if !units.get(*position).is_some_and(|unit| (0x30..=0x39).contains(unit)) => output.push(0),
        0x30..=0x39 => return None,
        0x78 => {
            let end = position.checked_add(2)?;
            output.push(hex_units(units.get(*position..end)?)? as u16);
            *position = end;
        }
        0x75 => unicode_escape(units, position, output)?,
        0x0a | 0x2028 | 0x2029 => {}
        0x0d => {
            if units.get(*position) == Some(&0x0a) {
                *position += 1;
            }
        }
        unit => output.push(unit),
    }
    Some(())
}
fn parse_string(source: &JsString) -> Option<JsString> {
    let units = source.units();
    let quote = *units.first()?;
    if !matches!(quote, 0x22 | 0x27) || units.len() < 2 {
        return None;
    }
    let mut position = 1;
    let mut output = Vec::new();
    while let Some(unit) = units.get(position) {
        position += 1;
        match *unit {
            unit if unit == quote => return (position == units.len()).then(|| JsString::from_utf16(output)),
            0x5c => append_escape(units, &mut position, &mut output)?,
            0x0a | 0x0d => return None,
            unit => output.push(unit),
        }
    }
    None
}

fn materialize_model_attributes(model: &JsValue) -> Result<JsValue, GeneratorError> {
    let mut result = object([]);
    for key in ["id", "name", "api", "provider", "baseUrl"] {
        if let Some(value) = values::property(Some(model), key)? {
            set(&mut result, key, string_value(model, key, &value)?);
        }
    }
    for key in ["headers", "compat"] {
        copy_json_field(model, &mut result, key)?;
    }
    set(
        &mut result,
        "reasoning",
        literal_value(model, "reasoning", field(model, "reasoning"))?,
    );
    copy_json_field(model, &mut result, "thinkingLevelMap")?;
    Ok(result)
}
fn copy_json_field(model: &JsValue, result: &mut JsValue, key: &str) -> Result<(), GeneratorError> {
    if let Some(value) = values::property(Some(model), key)?.filter(is_truthy) {
        set(result, key, value);
    }
    Ok(())
}
fn numeric_literal(model: &JsValue, key: &str, value: &JsValue) -> Result<JsValue, GeneratorError> {
    let emitted = literal_value(model, key, value)?;
    if !matches!(emitted, JsValue::Number(_)) {
        return Err(emission_error(
            model,
            key,
            "literal type does not match the native Model field",
        ));
    }
    Ok(emitted)
}
fn materialize_input(model: &JsValue) -> Result<JsValue, GeneratorError> {
    let values = field(model, "input").as_array().expect("fetchers create input arrays");
    values
        .iter()
        .map(|value| string_value(model, "input", value))
        .collect::<Result<Vec<_>, _>>()
        .map(JsValue::Array)
}
fn materialize_cost(model: &JsValue) -> Result<JsValue, GeneratorError> {
    let cost = values::property(Some(model), "cost")?;
    let mut result = object([]);
    for key in ["input", "output", "cacheRead", "cacheWrite"] {
        let value = values::property(cost.as_ref(), key)?.unwrap_or(JsValue::Null);
        let emitted = numeric_literal(model, &format!("cost.{key}"), &value)?;
        set(&mut result, key, emitted);
    }
    Ok(result)
}
pub(super) fn materialize(model: &JsValue) -> Result<JsValue, GeneratorError> {
    let mut result = materialize_model_attributes(model)?;
    set(&mut result, "input", materialize_input(model)?);
    set(&mut result, "cost", materialize_cost(model)?);
    for key in ["contextWindow", "autoCompactionThreshold", "maxTokens"] {
        if let Some(value) = values::property(Some(model), key)? {
            set(&mut result, key, numeric_literal(model, key, &value)?);
        }
    }
    pi_js_compat::js_json::from_value::<crate::types::Model>(&result)
        .map_err(|error| emission_error(model, "record", &error.to_string()))?;
    // JSON.stringify fields retain escaped lone units; only interpolated source
    // text crosses Node's UTF-8 write boundary.
    Ok(result)
}
