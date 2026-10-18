#[path = "differential_hash_sanitize/mod.rs"]
mod hash_sanitize_oracle;

#[test]
fn hash_and_surrogate_sanitization_match_the_pinned_typescript_source() {
    hash_sanitize_oracle::run();
}
