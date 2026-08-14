#[path = "differential_thinking_level_validator/mod.rs"]
mod thinking_level_validator_oracle;

#[test]
fn thinking_level_validator_matches_the_pinned_typescript_source() {
    thinking_level_validator_oracle::compare();
}
