#[path = "differential_ai_runtime/mod.rs"]
mod ai_runtime;

#[test]
fn ai_runtime_matches_the_pinned_typescript_source() {
    ai_runtime::run();
}
