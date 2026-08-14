#[path = "differential_current_session_version/mod.rs"]
mod current_session_version_oracle;

#[test]
fn current_session_version_matches_the_pinned_typescript_source() {
    current_session_version_oracle::compare();
}
