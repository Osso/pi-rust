#![recursion_limit = "512"]

#[path = "differential_settings_manager/mod.rs"]
mod settings_manager_oracle;

#[test]
fn settings_manager_matches_the_pinned_typescript_source() {
    settings_manager_oracle::run();
}
