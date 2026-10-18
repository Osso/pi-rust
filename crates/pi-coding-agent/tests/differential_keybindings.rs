#[path = "differential_keybindings/mod.rs"]
mod keybindings_oracle;

#[test]
fn keybindings_match_the_pinned_typescript_source() {
    keybindings_oracle::run();
}
