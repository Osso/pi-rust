#[path = "differential_harness_tools/mod.rs"]
mod harness_tools_oracle;

#[test]
fn harness_tools_match_the_pinned_typescript_source() {
    harness_tools_oracle::run();
}
