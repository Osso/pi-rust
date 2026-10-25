use crate::shared_oracle as oracle;

use pi_tui::native_modifiers::{ModifierKey, is_native_modifier_pressed};
use serde_json::json;

#[test]
fn linux_modifiers_match_frozen_source() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", "packages/tui/src");
    let response = oracle.request(&json!({"id":"native", "operation":"native_modifiers"}));
    assert_eq!(response["ok"], true, "{response}");
    let actual = [
        ModifierKey::Shift,
        ModifierKey::Command,
        ModifierKey::Control,
        ModifierKey::Option,
    ]
    .map(is_native_modifier_pressed);
    assert_eq!(json!(actual), response["output"]);
}
