//! Shared frozen `formatElapsedDuration` implementation for UI and extensions.

use pi_js_compat::number::js_number_to_string;
use pi_js_compat::string::utf16_len;

const SECOND_MS: f64 = 1000.0;
/// Smallest duration the UI shows; shorter durations still render as `Nms`.
pub const MIN_VISIBLE_ELAPSED_MS: f64 = SECOND_MS;
const MINUTE_SECONDS: f64 = 60.0;
const HOUR_SECONDS: f64 = 60.0 * MINUTE_SECONDS;

fn pad_two_digits(value: f64) -> String {
    let text = js_number_to_string(value);
    if utf16_len(&text) >= 2 {
        text
    } else {
        format!("0{text}")
    }
}

/// Sub-second durations use `ms`, then `s`, `m ss`, and `h mm`.
pub fn format_elapsed_duration(duration_ms: f64) -> String {
    let non_negative_duration_ms = if duration_ms.is_nan() {
        f64::NAN
    } else {
        duration_ms.max(0.0)
    };
    if non_negative_duration_ms < SECOND_MS {
        return format!("{}ms", js_number_to_string(non_negative_duration_ms.floor()));
    }
    let total_seconds = (non_negative_duration_ms / SECOND_MS).floor();
    if total_seconds < MINUTE_SECONDS {
        return format!("{}s", js_number_to_string(total_seconds));
    }
    let seconds = total_seconds % MINUTE_SECONDS;
    let total_minutes = (total_seconds / MINUTE_SECONDS).floor();
    if total_minutes < MINUTE_SECONDS {
        return format!("{}m {}s", js_number_to_string(total_minutes), pad_two_digits(seconds));
    }
    let hours = (total_seconds / HOUR_SECONDS).floor();
    let minutes = total_minutes % MINUTE_SECONDS;
    format!("{}h {}m", js_number_to_string(hours), pad_two_digits(minutes))
}
