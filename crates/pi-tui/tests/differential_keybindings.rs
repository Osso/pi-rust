#[path = "support/oracle.rs"]
mod oracle;

use oracle::TsOracle;
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
                        .map(|key| key.as_str().expect("binding key must be a string").to_string())
                        .collect();
                    (id.to_string(), keys)
                })
                .collect()
        })
        .unwrap_or_default();
    let manager = KeybindingsManager::new(tui_keybindings(), config);
    let definitions: Vec<String> = manager.get_resolved_bindings().into_iter().map(|(id, _)| id).collect();
    let mut keys = serde_json::Map::new();
    let mut resolved = serde_json::Map::new();
    for id in definitions {
        let id_keys = manager.get_keys(&id);
        resolved.insert(id.clone(), json!(id_keys));
        keys.insert(id.clone(), json!(id_keys));
    }
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
