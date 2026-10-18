#[path = "differential_uuid/mod.rs"]
mod uuid_oracle;

#[test]
fn uuidv7_layout_and_monotonicity_match_the_pinned_typescript_source() {
    uuid_oracle::run();
}
