//! Native modifier-key queries.
//!
//! The TypeScript module loads a platform-specific Node N-API addon on macOS
//! (`darwin-modifiers.node`) and returns
//! `false` everywhere else. The Rust port has no Node addon boundary, so it
//! reports `false`; see `docs/porting/traceability/native-modifiers.md`.

/// Modifier keys queried by the native helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKey {
    Shift,
    Command,
    Control,
    Option,
}

/// Whether the platform-native helper reports the modifier as pressed.
///
/// Returns `false`: the Rust port does not load the Node native addons. This
/// matches the TypeScript result on every platform where the addon is absent
/// (including Linux and failed loads).
pub fn is_native_modifier_pressed(_key: ModifierKey) -> bool {
    false
}
