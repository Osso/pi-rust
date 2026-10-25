//! Frozen runtime-mailbox-transcript.ts: scan every complete record without a UTF-8 JSON boundary.
use super::session_control_db::ControlDbError;
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::node_fs::{NodeReadApi, node_read_error_record};

pub const RUNTIME_MAILBOX_ACCEPTANCE_CUSTOM_TYPE: &str = "runtime_mailbox_acceptance";

/// Persisted refs are raw JS values; null throws and missing properties remain undefined.
pub fn mailbox_store_refs_equal(left: &JsValue, right: &JsValue) -> Result<bool, ControlDbError> {
    if !strict_equal(property(left, "sessionPath")?, property(right, "sessionPath")?) {
        return Ok(false);
    }
    Ok(strict_equal(
        property(left, "messageId")?,
        property(right, "messageId")?,
    ))
}

fn property<'a>(value: &'a JsValue, key: &str) -> Result<Option<&'a JsValue>, ControlDbError> {
    match value {
        JsValue::Null => Err(ControlDbError::new(format!(
            "Cannot read properties of null (reading '{key}')"
        ))),
        JsValue::Object(object) => Ok(object.get(&JsString::from(key))),
        _ => Ok(None),
    }
}

fn strict_equal(left: Option<&JsValue>, right: Option<&JsValue>) -> bool {
    match (left, right) {
        (None, None) | (Some(JsValue::Null), Some(JsValue::Null)) => true,
        (Some(JsValue::String(left)), Some(JsValue::String(right))) => left == right,
        (Some(JsValue::Bool(left)), Some(JsValue::Bool(right))) => left == right,
        (Some(JsValue::Number(left)), Some(JsValue::Number(right))) => left == right,
        (Some(left @ JsValue::Object(_)), Some(right @ JsValue::Object(_)))
        | (Some(left @ JsValue::Array(_)), Some(right @ JsValue::Array(_))) => std::ptr::eq(left, right),
        _ => false,
    }
}

fn string_equals(value: Option<&JsValue>, expected: &str) -> bool {
    matches!(value, Some(JsValue::String(value)) if value == &JsString::from(expected))
}

/// Return raw references, including lone-surrogate strings/keys and malformed array elements.
pub fn read_accepted_runtime_mailbox_store_refs(
    session_path: Option<&JsString>,
) -> Result<Vec<JsValue>, ControlDbError> {
    let Some(path) = session_path.filter(|path| !path.is_empty()) else {
        return Ok(Vec::new());
    };
    let bytes = match std::fs::read(js_json::encode_node_utf8(path)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            let record = node_read_error_record(&error, path, NodeReadApi::ReadFileSync);
            return Err(ControlDbError::from(NativeNodeError::from(record)));
        }
    };
    // This is the source's filesystem UTF-8 decode boundary, not a JSON-string conversion.
    let transcript = String::from_utf8_lossy(&bytes);
    let mut refs = Vec::new();
    let mut lines = transcript.split('\n').collect::<Vec<_>>();
    lines.pop(); // Even valid final JSON is not durable without LF.
    for line in lines {
        if pi_js_compat::string::js_trim(line).is_empty() {
            continue;
        }
        let entry =
            js_json::parse_lossless(line).map_err(|error| NativeNodeError::new("SyntaxError", error.into_message()))?;
        let kind = property(&entry, "type")?;
        let accepted = if string_equals(kind, "message") {
            property(&entry, "mailboxStoreRefs")?
        } else if string_equals(kind, "custom")
            && string_equals(property(&entry, "customType")?, RUNTIME_MAILBOX_ACCEPTANCE_CUSTOM_TYPE)
        {
            match property(&entry, "data")? {
                None | Some(JsValue::Null) => None,
                Some(data) => property(data, "mailboxStoreRefs")?,
            }
        } else {
            None
        };
        match accepted {
            Some(JsValue::Array(values)) => refs.extend(values.iter().cloned()),
            Some(JsValue::String(value)) => refs.extend(iterate_string(value)),
            None | Some(JsValue::Null) => {}
            _ => {
                return Err(ControlDbError::new(
                    "Spread syntax requires ...iterable[Symbol.iterator] to be a function",
                ));
            }
        }
    }
    Ok(refs)
}

/// JavaScript's string iterator groups valid surrogate pairs, retaining lone units.
fn iterate_string(value: &JsString) -> Vec<JsValue> {
    let units = value.units();
    let mut result = Vec::new();
    let mut index = 0;
    while index < units.len() {
        let width = if (0xd800..=0xdbff).contains(&units[index])
            && units
                .get(index + 1)
                .is_some_and(|unit| (0xdc00..=0xdfff).contains(unit))
        {
            2
        } else {
            1
        };
        result.push(JsValue::String(JsString::from_utf16(
            units[index..index + width].to_vec(),
        )));
        index += width;
    }
    result
}
