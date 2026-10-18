//! JavaScript runtime semantics shared by the Rust port.
//!
//! The modules here are the single source of truth for behaviour that the
//! TypeScript packages inherit from Node: `String.prototype.localeCompare`,
//! `Number`/`parseInt`/`parseFloat`/`Date.parse`, Base64 (`atob`, `btoa`,
//! `Buffer`), JavaScript string whitespace, and `minimatch`.

pub mod base64;
pub mod collator;
pub mod date;
pub mod minimatch;
pub mod number;
pub mod string;

pub use collator::{compare_in_locale, host_locale, locale_compare};
