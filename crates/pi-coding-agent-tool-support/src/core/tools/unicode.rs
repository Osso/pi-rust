//! Native `String.prototype.normalize` forms backed by ICU4X.

use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
use pi_js_compat::js_json::JsString;

/// `String.prototype.normalize("NFC")`.
pub fn normalize_nfc(text: &JsString) -> JsString {
    normalize_scalar_runs(text, |run| {
        ComposingNormalizerBorrowed::new_nfc().normalize_utf16(run).into_owned()
    })
}

/// `String.prototype.normalize("NFD")`.
pub fn normalize_nfd(text: &JsString) -> JsString {
    normalize_scalar_runs(text, |run| {
        DecomposingNormalizerBorrowed::new_nfd()
            .normalize_utf16(run)
            .into_owned()
    })
}

/// `String.prototype.normalize("NFKC")`.
pub fn normalize_nfkc(text: &JsString) -> JsString {
    normalize_scalar_runs(text, |run| {
        ComposingNormalizerBorrowed::new_nfkc()
            .normalize_utf16(run)
            .into_owned()
    })
}

/// `String.prototype.normalize("NFKD")`.
pub fn normalize_nfkd(text: &JsString) -> JsString {
    normalize_scalar_runs(text, |run| {
        DecomposingNormalizerBorrowed::new_nfkd()
            .normalize_utf16(run)
            .into_owned()
    })
}

// ICU4X replaces lone units. V8's ICU4C treats them as inert starters, so
// normalize only complete scalar runs and preserve each lone unit as a boundary.
fn normalize_scalar_runs(text: &JsString, normalize: impl Fn(&[u16]) -> Vec<u16>) -> JsString {
    let units = text.units();
    let mut output = Vec::with_capacity(units.len());
    let mut start = 0;
    let mut index = 0;
    while index < units.len() {
        if is_surrogate_pair(units, index) {
            index += 2;
            continue;
        }
        if (0xd800..=0xdfff).contains(&units[index]) {
            output.extend(normalize(&units[start..index]));
            output.push(units[index]);
            start = index + 1;
        }
        index += 1;
    }
    output.extend(normalize(&units[start..]));
    JsString::from_utf16(output)
}

fn is_surrogate_pair(units: &[u16], index: usize) -> bool {
    (0xd800..=0xdbff).contains(&units[index])
        && units
            .get(index + 1)
            .is_some_and(|next| (0xdc00..=0xdfff).contains(next))
}
