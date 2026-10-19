//! Experimental feature gate (`core/experimental.ts`).

use pi_ai::types::{ConstrainedSampling, ConstrainedSamplingConfig, JsonSchemaStrictness};

/// True only when `PI_EXPERIMENTAL` is exactly `1`; read on every call like the source.
pub fn are_experimental_features_enabled() -> bool {
    pi_js_compat::process_env::get("PI_EXPERIMENTAL").is_some_and(|value| value == "1")
}

/// `{ type: "json_schema", strict: "prefer" }` when experimental features are enabled.
pub fn get_experimental_tool_sampling() -> Option<ConstrainedSamplingConfig> {
    are_experimental_features_enabled().then_some(ConstrainedSamplingConfig::JsonSchema {
        strict: JsonSchemaStrictness::Prefer,
    })
}

/// The tool-level `constrainedSampling` value (`undefined` when disabled).
pub fn get_experimental_tool_constrained_sampling() -> Option<ConstrainedSampling> {
    get_experimental_tool_sampling().map(ConstrainedSampling::Config)
}
