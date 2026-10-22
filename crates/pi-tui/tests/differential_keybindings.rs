#[path = "support/oracle.rs"]
mod oracle;

use oracle::TsOracle;
use pi_js_compat::js_json::{JsString, JsValue, to_value};
use pi_tui::keybindings::{KeybindingsManager, tui_keybindings};
use serde_json::{Value, json};

fn rust_keybindings(user_bindings: &Value) -> Value {
    let config = user_bindings
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    let pair = entry.as_array().expect("binding entry must be a pair");
                    let id = pair[0].as_str().expect("binding id must be a string");
                    let keys = pair[1]
                        .as_array()
                        .expect("binding keys must be an array")
                        .iter()
                        .map(|key| JsString::from(key.as_str().expect("binding key must be a string")))
                        .collect();
                    (JsString::from(id), keys)
                })
                .collect()
        })
        .unwrap_or_default();
    let manager = KeybindingsManager::new(tui_keybindings(), config);
    let definitions: Vec<JsString> = manager.get_resolved_bindings().into_iter().map(|(id, _)| id).collect();
    let keys = JsValue::object(definitions.into_iter().map(|id| {
        let value = to_value(&manager.get_keys(&id)).unwrap();
        (id, value)
    }));
    let resolved = keys.clone();
    let conflicts: Vec<Value> = manager
        .get_conflicts()
        .into_iter()
        .map(|conflict| json!({"key": conflict.key, "keybindings": conflict.keybindings}))
        .collect();
    json!({
        "keys": keys,
        "resolved": resolved,
        "conflicts": conflicts,
    })
}

#[test]
fn keybinding_resolution_matches_typescript() {
    let mut oracle = TsOracle::spawn("tui-core-oracle.ts", "packages/tui/src");
    let configs = vec![
        json!([]),
        json!([["tui.input.submit", ["enter", "ctrl+enter"]]]),
        json!([["tui.select.up", ["up", "ctrl+p"]]]),
        json!([["tui.input.submit", ["ctrl+x"]], ["tui.select.confirm", ["ctrl+x"]]]),
        json!([["tui.select.up", ["ctrl+p", "ctrl+p", "up"]]]),
        json!([["not.a.binding", ["ctrl+x"]]]),
        json!([["tui.editor.cursorUp", ["ctrl+p"]], ["tui.select.up", ["ctrl+p"]]]),
    ];
    for user_bindings in configs {
        let response = oracle.request(&json!({
            "id": "keybindings",
            "operation": "keybindings",
            "userBindings": user_bindings,
        }));
        assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response}");
        let expected = &response["output"];
        let actual = rust_keybindings(&user_bindings);
        assert_eq!(
            actual["conflicts"], expected["conflicts"],
            "conflicts differ for {user_bindings}"
        );
        assert_eq!(actual["keys"], expected["keys"], "keys differ for {user_bindings}");
        assert_eq!(
            actual["resolved"], expected["resolved"],
            "resolved bindings differ for {user_bindings}"
        );
    }
}

#[test]
fn global_registry_holders_share_mutation_and_replacement_identity() {
    use pi_tui::keybindings::{get_keybindings, set_keybindings};
    let mut installed = KeybindingsManager::new(tui_keybindings(), Vec::new());
    set_keybindings(installed.clone());
    let mut first = get_keybindings();
    let second = get_keybindings();
    first.set_user_bindings(vec![("tui.input.submit".into(), vec!["ctrl+x".into()])]);
    assert!(second.matches("\x18", "tui.input.submit"));
    assert!(installed.matches("\x18", "tui.input.submit"));
    let shared = [
        first.get_keys("tui.input.submit"),
        second.get_keys("tui.input.submit"),
        installed.get_keys("tui.input.submit"),
    ];
    installed.set_user_bindings(Vec::new());
    assert_eq!(first.get_keys("tui.input.submit"), vec!["enter"]);
    let reset = first.get_keys("tui.input.submit");
    set_keybindings(KeybindingsManager::new(
        tui_keybindings(),
        vec![("tui.input.submit".into(), vec!["ctrl+y".into()])],
    ));
    assert_eq!(get_keybindings().get_keys("tui.input.submit"), vec!["ctrl+y"]);
    assert_eq!(first.get_keys("tui.input.submit"), vec!["enter"]);
    let actual = json!({"shared":shared,"reset":reset,"fresh":get_keybindings().get_keys("tui.input.submit"),"old":first.get_keys("tui.input.submit")});
    let mut oracle = TsOracle::spawn("tui-core-oracle.ts", "packages/tui/src");
    let expected = oracle.request(&json!({"id":"global", "operation":"keybinding_global"}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(actual, expected["output"]);
}
