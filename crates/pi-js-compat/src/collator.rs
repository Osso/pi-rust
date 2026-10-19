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

/// `new Intl.Collator(locale).compare(left, right)` for an explicit locale.
pub fn compare_in_locale(left: &str, right: &str, locale: &str) -> Ordering {
    build_collator(locale).compare(left, right)
}

thread_local! {
    /// `Collator` holds `Rc` payloads, so it is not `Sync`; one per thread.
    static HOST_COLLATOR: RefCell<CollatorBorrowed<'static>> =
        RefCell::new(build_collator(&host_locale()));
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
