#[path = "differential_model_runtime/mod.rs"]
mod model_runtime_oracle;

#[test]
fn model_runtime_composition_matches_the_pinned_typescript_source() {
    model_runtime_oracle::run();
}
