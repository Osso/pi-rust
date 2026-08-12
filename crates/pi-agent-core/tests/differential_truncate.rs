#[path = "differential_truncate/mod.rs"]
mod truncate_oracle;

#[test]
fn truncation_matches_the_pinned_typescript_source() {
    truncate_oracle::run();
}
