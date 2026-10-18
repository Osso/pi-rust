#[path = "../support/oracle.rs"]
mod oracle;

use std::fs;

use pi_coding_agent::core::keybindings::{KeybindingsManager, migrate_keybindings_config};
use serde_json::{Map, Value, json};
use tempfile::TempDir;

const KEYS: [&str; 5] = [
    "app.tool.background",
    "app.agent.select",
    "app.agent.slot9",
    "tui.select.confirm",
    "app.interrupt",
];

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-keybindings-oracle.ts",
        "packages/coding-agent/src/core/keybindings.ts",
    );

    compare_migrations(&mut oracle);
    compare_loaded_files(&mut oracle);
}

fn compare_migrations(oracle: &mut oracle::TsOracle) {
    let cases: Vec<Value> = vec![
        json!({"cursorUp": ["up", "ctrl+p"], "expandTools": "ctrl+x"}),
        json!({"expandTools": "ctrl+x", "app.tools.expand": "ctrl+y"}),
        json!({"z-extra": "ctrl+z", "cursorUp": "up", "a-extra": ["a", "b"]}),
        json!({"unknown": "ctrl+u", "interrupt": "ctrl+x", "selectConfirm": "enter"}),
        json!({"10": "digit", "2": "two", "alpha": "a"}),
    ];

    for (index, raw) in cases.iter().enumerate() {
        let response = oracle.request(&json!({"id": index, "operation": "migrate", "config": raw}));
        assert_eq!(response["ok"], true, "oracle failed: {response}");

        let object: Map<String, Value> = raw.as_object().expect("case object").clone();
        let actual = migrate_keybindings_config(&object);
        let actual_entries: Vec<Value> = actual.config.iter().map(|(key, value)| json!([key, value])).collect();
        assert_eq!(Value::Array(actual_entries), response["output"]["config"]);
        assert_eq!(Value::Bool(actual.migrated), response["output"]["migrated"]);
    }
}

fn compare_loaded_files(oracle: &mut oracle::TsOracle) {
    let cases: Vec<(&str, Option<&str>)> = vec![
        ("legacy", Some(r#"{"selectConfirm":"enter","interrupt":"ctrl+x"}"#)),
        (
            "mixed",
            Some(
                r#"{"expandTools":"ctrl+x","app.tools.expand":"ctrl+y","invalid":7,"partial":["a",3],"extra":"ctrl+q"}"#,
            ),
        ),
        ("invalid-json", Some("{ nope")),
        ("missing", None),
    ];

    for (index, (name, content)) in cases.iter().enumerate() {
        let temp = TempDir::new().expect("temp dir");
        if let Some(content) = content {
            fs::write(temp.path().join("keybindings.json"), content).unwrap();
        }
        let agent_dir = temp.path().to_string_lossy().into_owned();
        let response = oracle.request(&json!({
            "id": index,
            "operation": "load",
            "agentDir": &agent_dir,
            "keys": KEYS,
        }));
        assert_eq!(response["ok"], true, "oracle failed for {name}: {response}");

        let manager = KeybindingsManager::create(Some(temp.path()));
        let user_bindings: Vec<Value> = manager
            .get_user_bindings()
            .into_iter()
            .map(|(key, keys)| json!([key, keys]))
            .collect();
        assert_eq!(
            Value::Array(user_bindings),
            response["output"]["userBindings"],
            "{name}"
        );

        let effective: Vec<Value> = manager
            .get_effective_config()
            .into_iter()
            .map(|(key, keys)| json!([key, keys]))
            .collect();
        assert_eq!(Value::Array(effective), response["output"]["effective"], "{name}");

        let keys: Vec<Vec<String>> = KEYS.iter().map(|keybinding| manager.get_keys(keybinding)).collect();
        assert_eq!(json!(keys), response["output"]["keys"], "{name}");
    }
}
