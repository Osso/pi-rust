#[path = "../support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use std::fs;

use pi_coding_agent::core::keybindings::{KeybindingsManager, migrate_keybindings_config};
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use tempfile::TempDir;

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

        let object = JsValue::from(raw.clone()).as_object().expect("case object").clone();
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
        }));
        assert_eq!(response["ok"], true, "oracle failed for {name}: {response}");

        let manager = KeybindingsManager::create(Some(&JsString::from(
            (temp.path()).to_str().expect("fixture path must be UTF-8"),
        )))
        .unwrap();
        let effective = manager.get_effective_config();
        let ids: Vec<&JsString> = effective.iter().map(|(id, _)| id).collect();
        assert_eq!(json!(ids), response["output"]["ids"], "{name}");
        let keys: Vec<&Vec<JsString>> = effective.iter().map(|(_, keys)| keys).collect();
        assert_eq!(json!(keys), response["output"]["keys"], "{name}");
    }
}
