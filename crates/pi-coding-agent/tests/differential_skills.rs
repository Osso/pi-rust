#[path = "differential_skills/mod.rs"]
mod skills_oracle;

#[test]
fn skills_match_the_pinned_typescript_source() {
    skills_oracle::run();
}
