#[path = "differential_tool_output_persistence_constants/mod.rs"]
mod tool_output_persistence_constants_oracle;

#[test]
fn tool_output_persistence_constants_match_the_pinned_typescript_source() {
    tool_output_persistence_constants_oracle::compare();
}
