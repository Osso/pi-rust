#[path = "differential_config_value_references/mod.rs"]
mod config_value_reference_oracle;

#[test]
fn config_value_references_match_the_pinned_typescript_source() {
    config_value_reference_oracle::run();
}
