//! JavaScript-compatible timestamp formatting used by the schema.

use pi_js_compat::date::{js_now_millis, to_iso_string};

/// Formats Unix milliseconds exactly like `new Date(ms).toISOString()`.
pub fn iso_timestamp_from_millis(millis: i64) -> String {
    to_iso_string(millis)
}

/// Current wall-clock time as an ISO-8601 string.
pub fn iso_timestamp_now() -> String {
    iso_timestamp_from_millis(now_millis())
}

/// Current wall-clock time in Unix milliseconds (`Date.now()`).
pub fn now_millis() -> i64 {
    js_now_millis() as i64
}
