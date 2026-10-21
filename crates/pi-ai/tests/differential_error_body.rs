#[path = "differential_error_body/mod.rs"]
mod error_body_oracle;

#[test]
fn provider_error_normalization_matches_the_pinned_typescript_source() {
    error_body_oracle::run();
}

#[test]
fn ecmascript_trim_matches_source() {
    error_body_oracle::run_trim();
}

#[test]
fn json_stringify_matches_source() {
    error_body_oracle::run_stringify();
}

#[test]
fn body_and_status_selection_match_source() {
    error_body_oracle::run_selection();
}

#[test]
fn source_surrogate_split_is_an_open_public_string_boundary() {
    error_body_oracle::run_utf16_boundary();
}
