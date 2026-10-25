#[path = "differential_model_resolver/mod.rs"]
mod model_resolver_oracle;

#[test]
fn resolver_exact_matching_matches_frozen() {
    model_resolver_oracle::run("exact");
}
#[test]
fn resolver_parse_patterns_match_frozen() {
    model_resolver_oracle::run("parse");
}
#[test]
fn resolver_scope_patterns_match_frozen() {
    model_resolver_oracle::run("scope");
}
#[test]
fn resolver_cli_selection_matches_frozen() {
    model_resolver_oracle::run("cli");
}
#[test]
fn resolver_empty_model_matches_frozen() {
    model_resolver_oracle::run("cli_empty_model");
}
#[test]
fn resolver_blank_model_matches_frozen() {
    model_resolver_oracle::run("cli_blank_model");
}
#[test]
fn resolver_empty_provider_matches_frozen() {
    model_resolver_oracle::run("cli_empty_provider");
}
#[test]
fn resolver_casefold_provider_collision_matches_frozen() {
    model_resolver_oracle::run("cli_casefold_provider");
}
#[test]
fn resolver_default_record_matches_frozen() {
    model_resolver_oracle::run("defaults");
}
#[test]
fn resolver_restore_console_and_models_match_frozen() {
    model_resolver_oracle::run("restore");
}

#[test]
#[ignore = "child process probe"]
fn resolver_restore_probe_child() {
    model_resolver_oracle::restore_child();
}

#[test]
fn resolver_initial_selection_matches_frozen() {
    model_resolver_oracle::run("initial");
}
