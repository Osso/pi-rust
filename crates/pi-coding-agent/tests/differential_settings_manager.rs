#![recursion_limit = "512"]

#[path = "differential_settings_manager/mod.rs"]
mod settings_manager_oracle;

#[test]
fn settings_manager_matches_the_pinned_typescript_source() {
    settings_manager_oracle::run();
}

#[test]
fn settings_manager_bidirectional_stale_writers_preserve_unknown_fields() {
    settings_manager_oracle::bidirectional();
}

#[test]
fn settings_manager_preserves_queued_save_ordering() {
    settings_manager_oracle::queued_write_ordering();
}
