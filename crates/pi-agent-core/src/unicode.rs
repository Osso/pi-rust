//! `String.prototype.normalize` forms backed by ICU4X.
//!
//! The workspace uses `icu_normalizer` as the single Unicode normalization
//! implementation, matching Node's ICU data family.

use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};

/// `String.prototype.normalize("NFC")`.
pub fn normalize_nfc(text: &str) -> String {
    ComposingNormalizerBorrowed::new_nfc().normalize(text).into_owned()
}

/// `String.prototype.normalize("NFD")`.
pub fn normalize_nfd(text: &str) -> String {
    DecomposingNormalizerBorrowed::new_nfd().normalize(text).into_owned()
}

/// `String.prototype.normalize("NFKC")`.
pub fn normalize_nfkc(text: &str) -> String {
    ComposingNormalizerBorrowed::new_nfkc().normalize(text).into_owned()
}

/// `String.prototype.normalize("NFKD")`.
pub fn normalize_nfkd(text: &str) -> String {
    DecomposingNormalizerBorrowed::new_nfkd().normalize(text).into_owned()
}
