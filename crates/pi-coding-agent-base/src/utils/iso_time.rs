//! ISO-8601 timestamps: `Date#toISOString` and `new Date().toISOString()`.

use pi_js_compat::date::{js_now_millis, to_iso_string};

/// `new Date().toISOString()`.
pub fn now_iso_string() -> String {
    format_iso_millis(js_now_millis() as i64)
}

/// `new Date(epoch_ms).toISOString()`.
pub fn format_iso_millis(epoch_ms: i64) -> String {
    to_iso_string(epoch_ms)
}
