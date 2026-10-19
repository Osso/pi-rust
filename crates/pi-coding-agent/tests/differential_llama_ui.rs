//! Differential tests for `extensions/llama/ui.ts`.

#[path = "differential_llama_ui/mod.rs"]
mod differential_llama_ui;

#[test]
fn llama_ui_matches_the_pinned_typescript_source() {
    differential_llama_ui::run();
}
