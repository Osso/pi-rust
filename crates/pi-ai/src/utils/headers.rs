use std::collections::BTreeMap;

use indexmap::IndexMap;

use crate::types::ProviderHeaders;

/// Convert header entries to a record, matching `Headers.entries()` normalization:
/// names are lowercased, surrounding HTTP whitespace is trimmed, and duplicate
/// names are joined with `", "`.
pub fn headers_to_record(headers: impl IntoIterator<Item = (String, String)>) -> BTreeMap<String, String> {
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
    record
}

fn http_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r')
}

/// Convert `ProviderHeaders` to a record, dropping explicit `None` suppressions.
pub fn provider_headers_to_record(headers: Option<&ProviderHeaders>) -> Option<IndexMap<String, String>> {
    let headers = headers?;
    let record: IndexMap<String, String> = headers
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|value| (key.clone(), value.clone())))
        .collect();
    if record.is_empty() { None } else { Some(record) }
}
