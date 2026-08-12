#[path = "differential_auth_command/mod.rs"]
mod auth_command_oracle;

#[test]
fn auth_command_metadata_matches_the_pinned_typescript_source() {
    auth_command_oracle::run();
}
