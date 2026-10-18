#[path = "differential_error_body/mod.rs"]
mod error_body_oracle;

#[test]
fn provider_error_normalization_matches_the_pinned_typescript_source() {
    error_body_oracle::run();
}
