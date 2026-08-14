#[path = "differential_source_info/mod.rs"]
mod source_info_oracle;

#[test]
fn source_info_matches_the_pinned_typescript_source() {
    source_info_oracle::compare();
}
