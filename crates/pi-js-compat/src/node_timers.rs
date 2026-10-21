//! Node's numeric `setTimeout` delay normalization, after caller-side ToNumber.

const TIMEOUT_MAX_MS: f64 = 2_147_483_647.0;

/// Delays outside Node's inclusive timer range become 1ms; valid fractions remain.
/// Warning emission and operating-system scheduling precision are not modeled.
pub fn normalize_timeout_delay(delay_ms: f64) -> f64 {
    if (1.0..=TIMEOUT_MAX_MS).contains(&delay_ms) {
        delay_ms
    } else {
        1.0
    }
}
