#[path = "differential_cli_mode/mod.rs"]
mod cli_mode_oracle;

#[test]
fn cli_mode_matches_the_pinned_typescript_source() {
    cli_mode_oracle::compare();
}
