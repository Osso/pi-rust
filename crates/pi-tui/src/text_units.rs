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

/// Recognize the ASCII keyboard protocol grammar without changing native input.
/// This projection is never component text state, a rendered line, or a fallback.
pub fn ascii_protocol(data: &JsString) -> Option<String> {
    data.units()
        .iter()
        .map(|unit| u8::try_from(*unit).ok().filter(u8::is_ascii).map(char::from))
        .collect()
}

/// Repeat native units, including pairs formed across repetition boundaries.
pub fn repeat(text: &JsString, count: usize) -> JsString {
    JsString::from_utf16(text.units().repeat(count))
}

/// Reverse units (not scalars) to reuse the shared linear-time substring search.
pub fn rfind(text: &JsString, needle: &JsString) -> Option<usize> {
    if needle.len() > text.len() {
        return None;
    }
    let reversed = JsString::from_utf16(text.units().iter().rev().copied().collect());
    let pattern = JsString::from_utf16(needle.units().iter().rev().copied().collect());
    reversed.find(pattern).map(|index| text.len() - index - needle.len())
}

/// Remove one literal marker, preserving every unmatched native unit.
pub fn remove_first(text: &JsString, pattern: impl Into<JsString>) -> JsString {
    let pattern = pattern.into();
    let Some(index) = text.find(&pattern) else {
        return text.clone();
    };
    text.slice(0, index) + text.slice(index + pattern.len(), text.len())
}
