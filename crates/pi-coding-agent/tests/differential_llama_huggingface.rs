//! Differential tests for `extensions/llama/huggingface.ts`.

#[path = "differential_llama_huggingface/mod.rs"]
mod differential_llama_huggingface;

#[test]
fn llama_huggingface_matches_the_pinned_typescript_source() {
    differential_llama_huggingface::run();
}
