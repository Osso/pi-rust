//! Differential tests for `extensions/llama/provider.ts`.

#[path = "differential_llama_provider/mod.rs"]
mod differential_llama_provider;

#[test]
fn llama_provider_matches_the_pinned_typescript_source() {
    differential_llama_provider::run();
}
