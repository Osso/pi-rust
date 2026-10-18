#[path = "differential_json_parse/mod.rs"]
mod json_parse_oracle;

#[test]
fn json_repair_and_partial_parsing_match_the_pinned_typescript_source() {
    json_parse_oracle::run();
}
