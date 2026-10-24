use std::collections::BTreeMap;

use indexmap::IndexMap;

use pi_js_compat::js_json::{self, JsString, JsValue};

use crate::types::ProviderHeaders;

/// Header names are opaque protocol tokens; reject unpaired units before scalar-key decoding.
pub fn validate_opaque_header_names(headers: Option<&JsValue>, context: &str) -> Result<(), String> {
    let Some(headers) = headers.and_then(JsValue::as_object) else {
        return Ok(());
    };
    for (name, _) in js_json::object_entries_lossless(headers) {
        if name.to_utf8().is_err() {
            let name = js_json::stringify_lossless(&JsValue::String(name.clone()));
            return Err(format!(
                "{context}: header name {name} contains an unpaired UTF-16 unit"
            ));
        }
    }
    Ok(())
}

/// Convert header entries to a record, matching `Headers.entries()` normalization:
/// names are lowercased, surrounding HTTP whitespace is trimmed, and duplicate
/// names are joined with `", "`. Keys retain the lowercase-sorted order of
/// WHATWG `Headers.entries()`, not the insertion order of the raw entries.
pub fn headers_to_record(headers: impl IntoIterator<Item = (String, String)>) -> IndexMap<String, String> {
    let mut record = BTreeMap::new();
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        let value = value.trim_matches(http_whitespace);
        record
            .entry(name)
            .and_modify(|existing: &mut String| {
                existing.push_str(", ");
                existing.push_str(value);
            })
            .or_insert_with(|| value.to_owned());
    }
    record.into_iter().collect()
}

fn http_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r')
}

/// Convert `ProviderHeaders` to a record, dropping explicit `None` suppressions.
pub fn provider_headers_to_record(headers: Option<&ProviderHeaders>) -> Option<IndexMap<String, JsString>> {
    let headers = headers?;
    let record: IndexMap<String, JsString> = headers
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|value| (key.clone(), value.clone())))
        .collect();
    if record.is_empty() { None } else { Some(record) }
}
