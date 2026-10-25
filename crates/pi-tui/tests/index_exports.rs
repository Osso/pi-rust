//! Package-root export contracts (`packages/tui/src/index.ts`).
//!
//! The `Key` map and the `Marked` facade are compared against the pinned
//! TypeScript module through the shared oracle.

use crate::shared_oracle as oracle;

use pi_tui::keys::Key;
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

fn constants() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("escape", "ESCAPE", Key::ESCAPE),
        ("esc", "ESC", Key::ESC),
        ("enter", "ENTER", Key::ENTER),
        ("return", "RETURN", Key::RETURN),
        ("tab", "TAB", Key::TAB),
        ("space", "SPACE", Key::SPACE),
        ("backspace", "BACKSPACE", Key::BACKSPACE),
        ("delete", "DELETE", Key::DELETE),
        ("insert", "INSERT", Key::INSERT),
        ("clear", "CLEAR", Key::CLEAR),
        ("home", "HOME", Key::HOME),
        ("end", "END", Key::END),
        ("pageUp", "PAGE_UP", Key::PAGE_UP),
        ("pageDown", "PAGE_DOWN", Key::PAGE_DOWN),
        ("up", "UP", Key::UP),
        ("down", "DOWN", Key::DOWN),
        ("left", "LEFT", Key::LEFT),
        ("right", "RIGHT", Key::RIGHT),
        ("f1", "F1", Key::F1),
        ("f2", "F2", Key::F2),
        ("f3", "F3", Key::F3),
        ("f4", "F4", Key::F4),
        ("f5", "F5", Key::F5),
        ("f6", "F6", Key::F6),
        ("f7", "F7", Key::F7),
        ("f8", "F8", Key::F8),
        ("f9", "F9", Key::F9),
        ("f10", "F10", Key::F10),
        ("f11", "F11", Key::F11),
        ("f12", "F12", Key::F12),
        ("backtick", "BACKTICK", Key::BACKTICK),
        ("hyphen", "HYPHEN", Key::HYPHEN),
        ("equals", "EQUALS", Key::EQUALS),
        ("leftbracket", "LEFT_BRACKET", Key::LEFT_BRACKET),
        ("rightbracket", "RIGHT_BRACKET", Key::RIGHT_BRACKET),
        ("backslash", "BACKSLASH", Key::BACKSLASH),
        ("semicolon", "SEMICOLON", Key::SEMICOLON),
        ("quote", "QUOTE", Key::QUOTE),
        ("comma", "COMMA", Key::COMMA),
        ("period", "PERIOD", Key::PERIOD),
        ("slash", "SLASH", Key::SLASH),
        ("exclamation", "EXCLAMATION", Key::EXCLAMATION),
        ("at", "AT", Key::AT),
        ("hash", "HASH", Key::HASH),
        ("dollar", "DOLLAR", Key::DOLLAR),
        ("percent", "PERCENT", Key::PERCENT),
        ("caret", "CARET", Key::CARET),
        ("ampersand", "AMPERSAND", Key::AMPERSAND),
        ("asterisk", "ASTERISK", Key::ASTERISK),
        ("leftparen", "LEFT_PAREN", Key::LEFT_PAREN),
        ("rightparen", "RIGHT_PAREN", Key::RIGHT_PAREN),
        ("underscore", "UNDERSCORE", Key::UNDERSCORE),
        ("plus", "PLUS", Key::PLUS),
        ("pipe", "PIPE", Key::PIPE),
        ("tilde", "TILDE", Key::TILDE),
        ("leftbrace", "LEFT_BRACE", Key::LEFT_BRACE),
        ("rightbrace", "RIGHT_BRACE", Key::RIGHT_BRACE),
        ("colon", "COLON", Key::COLON),
        ("lessthan", "LESS_THAN", Key::LESS_THAN),
        ("greaterthan", "GREATER_THAN", Key::GREATER_THAN),
        ("question", "QUESTION", Key::QUESTION),
    ]
}

type ModifierCall = (
    &'static str,
    fn(pi_js_compat::js_json::JsString) -> pi_js_compat::js_json::JsString,
    &'static str,
);

fn modifier_calls() -> Vec<ModifierCall> {
    vec![
        ("ctrl", Key::ctrl, "c"),
        ("shift", Key::shift, "tab"),
        ("alt", Key::alt, "x"),
        ("super", Key::super_key, "k"),
        ("ctrlShift", Key::ctrl_shift, "p"),
        ("shiftCtrl", Key::shift_ctrl, "p"),
        ("ctrlAlt", Key::ctrl_alt, "x"),
        ("altCtrl", Key::alt_ctrl, "x"),
        ("shiftAlt", Key::shift_alt, "x"),
        ("altShift", Key::alt_shift, "x"),
        ("ctrlSuper", Key::ctrl_super, "k"),
        ("superCtrl", Key::super_ctrl, "k"),
        ("shiftSuper", Key::shift_super, "k"),
        ("superShift", Key::super_shift, "k"),
        ("altSuper", Key::alt_super, "k"),
        ("superAlt", Key::super_alt, "k"),
        ("ctrlShiftAlt", Key::ctrl_shift_alt, "x"),
        ("ctrlShiftSuper", Key::ctrl_shift_super, "k"),
    ]
}

#[test]
fn key_map_matches_the_pinned_typescript_object() {
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);

    let mut cases: Vec<Value> = constants().iter().map(|(name, _, _)| json!({ "name": name })).collect();
    cases.extend(
        modifier_calls()
            .iter()
            .map(|(name, _, argument)| json!({ "name": name, "args": [argument] })),
    );
    let response = oracle.request(&json!({
        "id": "key-constants",
        "operation": "keys_constants",
        "cases": cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let expected_constants = response["output"]["constants"].as_object().expect("constants object");
    for (source_name, rust_name, actual) in constants() {
        assert_eq!(
            expected_constants.get(source_name).and_then(Value::as_str),
            Some(actual),
            "Key::{rust_name} must match Key.{source_name}"
        );
    }

    let expected_calls = response["output"]["calls"].as_object().expect("calls object");
    for (source_name, call, argument) in modifier_calls() {
        assert_eq!(
            expected_calls
                .get(source_name)
                .and_then(Value::as_str)
                .map(pi_js_compat::js_json::JsString::from),
            Some(call(argument.into())),
            "Key::{source_name} must match"
        );
    }
}

use crate::shared_virtual_terminal as virtual_terminal;

#[test]
fn frozen_root_tui_constructs_and_renders() {
    use pi_tui::tui::TuiExt;
    let (terminal, handle) = virtual_terminal::concrete(20, 5);
    let tui = pi_tui::TUI::new(handle);
    tui.add_child(pi_tui::tui::component(pi_tui::Text::new("root", 0, 0)));
    tui.start().unwrap();
    tui.flush_pending().unwrap();
    assert!(terminal.borrow().viewport().iter().any(|line| line.trim() == "root"));
    tui.stop().unwrap();
}
