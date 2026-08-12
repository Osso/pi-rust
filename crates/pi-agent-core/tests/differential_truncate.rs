#[path = "differential_truncate/mod.rs"]
mod truncate_oracle;

#[test]
fn agent_utilities_match_the_pinned_typescript_source() {
    truncate_oracle::run();
}
