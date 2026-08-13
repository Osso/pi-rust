#[path = "differential_skill_block/mod.rs"]
mod skill_block_oracle;

#[test]
fn skill_block_matches_the_pinned_typescript_source() {
    skill_block_oracle::run();
}
