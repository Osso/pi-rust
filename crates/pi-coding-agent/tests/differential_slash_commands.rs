#[path = "differential_slash_commands/mod.rs"]
mod slash_commands_oracle;

#[test]
fn slash_commands_match_the_pinned_typescript_source() {
    slash_commands_oracle::run();
}
