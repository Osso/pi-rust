#[path = "differential_json_event/mod.rs"]
mod json_event_oracle;

#[test]
fn json_event_matches_the_pinned_typescript_source() {
    json_event_oracle::run();
}
