//! `String.prototype.localeCompare` / `Intl.Collator` ordering.

use std::cell::RefCell;
use std::cmp::Ordering;

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::Locale;

/// `String.prototype.localeCompare` in the host locale, like Node.
pub fn locale_compare(left: &str, right: &str) -> Ordering {
    HOST_COLLATOR.with(|collator| collator.borrow().compare(left, right))
}

/// Compare lossless JavaScript UTF-16 strings in the host locale.
pub fn locale_compare_utf16(left: &[u16], right: &[u16]) -> Ordering {
    // UChar::from(Vec<u16>) adopts code units without repairing unpaired surrogates.
    let left = rust_icu_ustring::UChar::from(left.to_vec());
    let right = rust_icu_ustring::UChar::from(right.to_vec());
    HOST_UTF16_COLLATOR.with(|collator| collator.strcoll(&left, &right))
}

/// `new Intl.Collator(locale).compare(left, right)` for an explicit locale.
pub fn compare_in_locale(left: &str, right: &str, locale: &str) -> Ordering {
    build_collator(locale).compare(left, right)
}

thread_local! {
    /// `Collator` holds `Rc` payloads, so it is not `Sync`; one per thread.
    static HOST_COLLATOR: RefCell<CollatorBorrowed<'static>> =
        RefCell::new(build_collator(&host_locale()));
    // Node's ICU4C supports UTF-16 code units outside Rust's Unicode-scalar string domain.
    static HOST_UTF16_COLLATOR: rust_icu_ucol::UCollator =
        rust_icu_ucol::UCollator::try_from(host_locale().as_str())
            .expect("ICU4C must initialize the host-locale UTF-16 collator");
}

fn build_collator(locale: &str) -> CollatorBorrowed<'static> {
    let locale: Locale = locale
        .parse()
        .unwrap_or_else(|_| "und".parse().expect("und is a valid locale"));
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(CollatorPreferences::from(&locale), options).unwrap_or_else(|_| {
        Collator::try_new(CollatorPreferences::default(), CollatorOptions::default()).expect("root collator")
    })
}

/// ICU's default locale comes from the environment; so does Node's.
pub fn host_locale() -> String {
    let raw = crate::process_env::get("LC_ALL")
        .filter(|value| !value.is_empty())
        .or_else(|| crate::process_env::get("LC_COLLATE").filter(|value| !value.is_empty()))
        .or_else(|| crate::process_env::get("LANG").filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "und".to_owned());
    let language = raw.split(['.', '@']).next().unwrap_or("und");
    let normalized = language.replace('_', "-");
    if normalized.is_empty() {
        "und".to_owned()
    } else {
        normalized
    }
}
