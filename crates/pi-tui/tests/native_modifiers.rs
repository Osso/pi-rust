use pi_tui::native_modifiers::{ModifierKey, is_native_modifier_pressed};

#[test]
fn no_native_helper_is_available_in_the_rust_port() {
    for key in [
        ModifierKey::Shift,
        ModifierKey::Command,
        ModifierKey::Control,
        ModifierKey::Option,
    ] {
        assert!(!is_native_modifier_pressed(key));
    }
}
