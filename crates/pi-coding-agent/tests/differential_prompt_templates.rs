#[path = "differential_prompt_templates/mod.rs"]
mod prompt_templates_oracle;

#[test]
fn prompt_templates_match_the_pinned_typescript_source() {
    prompt_templates_oracle::run();
}
