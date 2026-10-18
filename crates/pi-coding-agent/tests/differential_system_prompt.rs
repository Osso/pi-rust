#[path = "differential_system_prompt/mod.rs"]
mod system_prompt_oracle;

#[test]
fn system_prompt_matches_the_pinned_typescript_source() {
    system_prompt_oracle::run();
}
