//! Frozen `core/retry-delay.ts`: capped exponential backoff with symmetric jitter.

const RETRY_JITTER_RATIO: f64 = 0.2;
const RETRY_BACKOFF_FACTOR: f64 = 2.0;

pub fn calculate_retry_delay_ms(attempt: f64, base_delay_ms: f64, max_backoff_ms: f64, random_value: f64) -> f64 {
    if base_delay_ms == 0.0 {
        return 0.0;
    }
    let exponential_delay_ms = js_min(max_backoff_ms, base_delay_ms * RETRY_BACKOFF_FACTOR.powf(attempt - 1.0));
    let jitter_factor = 1.0 - RETRY_JITTER_RATIO + random_value * (2.0 * RETRY_JITTER_RATIO);
    let jittered_delay_ms = exponential_delay_ms * jitter_factor;
    js_min(max_backoff_ms, js_round(jittered_delay_ms))
}

fn js_min(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        return f64::NAN;
    }
    let both_zero = left == 0.0 && right == 0.0;
    let has_negative_zero = left.is_sign_negative() || right.is_sign_negative();
    if both_zero && has_negative_zero {
        -0.0
    } else {
        left.min(right)
    }
}

fn js_round(value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let floor = value.floor();
    let rounded = if value - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 {
        0.0_f64.copysign(value)
    } else {
        rounded
    }
}
