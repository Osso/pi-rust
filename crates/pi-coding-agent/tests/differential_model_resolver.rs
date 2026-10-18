#[path = "differential_model_resolver/mod.rs"]
mod model_resolver_oracle;

#[test]
fn model_resolver_matches_the_pinned_typescript_source() {
    model_resolver_oracle::run();
}
