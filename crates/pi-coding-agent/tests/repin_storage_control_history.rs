use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{read_or_migrate_prompt_history, read_prompt_history};
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};

#[test]
fn prompt_migration_keeps_js_slice_limits_and_the_complete_persisted_history() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let mut case = 0;
    for legacy in [
        vec![" first ", "second", "third", "fourth"],
        vec![" ", "\t", "\n", "\r"],
    ] {
        for limit in [-2, -1, 0, 1, 2, 6] {
            let source_db = root.path().join(format!("source{case}.sqlite"));
            let native_db = JsString::from(root.path().join(format!("native{case}.sqlite")).to_str().unwrap());
            case += 1;
            let expected = source.request(&json!({"id":1,"steps":[
                {"op":"call","args":["readOrMigratePromptHistory",[source_db,legacy,limit]]},
                {"op":"call","args":["readPromptHistory",[source_db,-1]]},
                {"op":"call","args":["readOrMigratePromptHistory",[source_db,legacy,limit]]}
            ]}));
            assert_eq!(expected["ok"], true, "{expected}");
            let native_legacy: Vec<JsString> = legacy.iter().map(|entry| JsString::from(*entry)).collect();
            let first = read_or_migrate_prompt_history(&native_db, &native_legacy, limit).unwrap();
            let stored = read_prompt_history(&native_db, -1).unwrap();
            let second = read_or_migrate_prompt_history(&native_db, &native_legacy, limit).unwrap();
            let actual = Value::Array(
                [first, stored, second]
                    .into_iter()
                    .map(|value| json!({"ok":true,"value":value}))
                    .collect(),
            );
            assert_eq!(
                JsValue::from(actual),
                JsValue::from(expected["output"].clone()),
                "limit {limit}, legacy {legacy:?}"
            );
        }
    }
}
