#[path = "differential_truncation_constants/mod.rs"]
mod truncation_constants_oracle;

#[test]
fn coding_agent_truncation_constants_match_the_pinned_typescript_source() {
    truncation_constants_oracle::compare();
}
