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
fn resolver_default_record_matches_frozen() {
    model_resolver_oracle::run("defaults");
}
#[test]
fn resolver_initial_selection_matches_frozen() {
    model_resolver_oracle::run("initial");
}
