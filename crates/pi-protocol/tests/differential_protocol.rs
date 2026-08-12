#[path = "differential_protocol/mod.rs"]
mod protocol_oracle;

#[test]
fn framing_matches_the_pinned_typescript_oracle() {
    protocol_oracle::run();
}
