//! JavaScript runtime semantics shared by the Rust port.
//!
//! The modules here are the single source of truth for behaviour that the
//! TypeScript packages inherit from Node: `String.prototype.localeCompare`,
//! `Number`/`parseFloat`/`Date.parse`, Base64 (`atob`, `btoa`, `Buffer`), and
//! JavaScript string whitespace.

pub mod collator;
pub mod minimatch;
pub mod string;

pub use collator::{compare_in_locale, host_locale, locale_compare};
