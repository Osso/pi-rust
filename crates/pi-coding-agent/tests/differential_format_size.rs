#[path = "differential_format_size/mod.rs"]
mod format_size_oracle;

#[test]
fn coding_agent_format_size_matches_the_pinned_typescript_source() {
    format_size_oracle::compare();
}
