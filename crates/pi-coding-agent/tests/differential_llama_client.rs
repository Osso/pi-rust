//! Differential tests for `extensions/llama/client.ts` against the pinned TypeScript source.

#[path = "differential_llama_client/mod.rs"]
mod differential_llama_client;

#[test]
fn llama_client_matches_the_pinned_typescript_source() {
    differential_llama_client::run();
}
