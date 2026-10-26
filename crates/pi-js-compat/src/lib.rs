//! JavaScript runtime semantics shared by the Rust port.
//!
//! The modules here are the single source of truth for behaviour that the
//! TypeScript packages inherit from Node: `String.prototype.localeCompare`,
//! `Number`/`parseInt`/`parseFloat`/`Date.parse`, Base64 (`atob`, `btoa`,
//! `Buffer`), JavaScript string whitespace, and `minimatch`.

pub mod abort;
pub mod array_sort;
pub mod base64;
pub mod callback_error;
pub mod collator;
pub mod date;
pub mod js_format;
pub mod js_json;
mod js_json_macros;
pub mod minimatch;
pub mod node_child_process;
pub mod node_error;
pub mod node_fs;
pub mod node_ignore;
pub mod node_os;
pub mod node_readline;
pub mod node_timers;
pub mod node_url;
pub mod number;
pub mod process_cwd;
pub mod process_env;
pub mod reminder_time;
pub mod string;
pub mod text_decoder;

pub use collator::{compare_in_locale, host_locale, locale_compare};

pub mod js_regex;
