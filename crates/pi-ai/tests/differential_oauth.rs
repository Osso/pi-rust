#[path = "differential_oauth/mod.rs"]
mod oauth_oracle;

#[test]
fn oauth_pages_and_pkce_match_the_pinned_typescript_source() {
    oauth_oracle::run();
}
