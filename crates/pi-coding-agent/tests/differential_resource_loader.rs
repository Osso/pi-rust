#[path = "differential_resource_loader/mod.rs"]
mod resource_loader_oracle;

#[test]
fn resource_loader_rule_helpers_match_the_pinned_typescript_source() {
    resource_loader_oracle::run();
}

#[test]
fn load_project_context_files_matches_the_pinned_typescript_source() {
    resource_loader_oracle::run_context_files();
}
