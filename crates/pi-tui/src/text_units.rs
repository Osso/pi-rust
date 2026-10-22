//! Native UTF-16 grapheme operations. No terminal encoding occurs here.

use icu_segmenter::GraphemeClusterSegmenter;
use pi_js_compat::js_json::JsString;

/// Owned segments retain the original units, including unmatched surrogates.
pub fn grapheme_indices(text: &JsString) -> impl DoubleEndedIterator<Item = (usize, JsString)> {
    let boundaries: Vec<usize> = GraphemeClusterSegmenter::new().segment_utf16(text.units()).collect();
    boundaries
        .windows(2)
        .map(|pair| (pair[0], text.slice(pair[0], pair[1])))
        .collect::<Vec<_>>()
        .into_iter()
}

pub fn graphemes(text: &JsString) -> impl DoubleEndedIterator<Item = JsString> {
    grapheme_indices(text).map(|(_, segment)| segment)
}

/// Property checks can inspect scalar values; unmatched units remain in the
/// authoritative string and must be classified explicitly by the caller.
pub fn scalar_values(text: &JsString) -> impl Iterator<Item = char> + '_ {
    char::decode_utf16(text.units().iter().copied()).filter_map(Result::ok)
}

pub fn concat(parts: impl IntoIterator<Item = impl Into<JsString>>) -> JsString {
    JsString::join(parts, "")
}

pub fn clear(text: &mut JsString) {
    *text = JsString::default();
}

pub fn insert(text: &mut JsString, index: usize, value: impl Into<JsString>) {
    text.replace_range(index..index, value);
}

pub fn truncate(text: &mut JsString, length: usize) {
    *text = text.slice(0, length);
}
