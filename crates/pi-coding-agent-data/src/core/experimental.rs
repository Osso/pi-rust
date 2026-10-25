//! Experimental feature gate (`core/experimental.ts`).

/// True only when `PI_EXPERIMENTAL` is exactly `1`; read on every call like the source.
pub fn are_experimental_features_enabled() -> bool {
    pi_js_compat::process_env::get("PI_EXPERIMENTAL").is_some_and(|value| value == "1")
}
