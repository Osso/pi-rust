#[path = "differential_default_thinking_level/mod.rs"]
mod default_thinking_level_oracle;

#[test]
fn default_thinking_level_matches_the_pinned_typescript_source() {
    default_thinking_level_oracle::compare();
}
