use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{
    WritableSessionMetadata, archive_sessions_older_than, list_session_metadata, read_or_migrate_prompt_history,
    read_prompt_history, write_session_metadata,
};
use pi_js_compat::js_json::{JsString, JsValue, from_value};
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

#[test]
fn archive_candidates_use_utf16_tie_order_without_archiving_rows() {
    let root = tempfile::tempdir().unwrap();
    let source_db = root.path().join("source.sqlite");
    let native_db = JsString::from(root.path().join("native.sqlite").to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let mut steps = Vec::new();
    for (name, modified, archived, subagent) in [
        ("Ａ", "2020-02-01T00:00:00.000Z", None, false),
        ("𠮷野", "2020-02-01T00:00:00.000Z", None, false),
        ("early", "2020-01-01T00:00:00.000Z", None, false),
        ("cutoff", "2020-03-01T00:00:00.000Z", None, false),
        (
            "archived",
            "2020-01-01T00:00:00.000Z",
            Some("2020-02-01T00:00:00.000Z"),
            false,
        ),
        ("child", "2020-01-01T00:00:00.000Z", None, true),
    ] {
        let mut value = json!({"sessionPath":format!("/fixture/{name}.jsonl"),"id":name,"cwd":"/fixture","createdAt":"2020-01-01T00:00:00.000Z","modifiedAt":modified,"messageCount":1,"firstMessage":"hello","isSubagent":subagent});
        if let Some(archived) = archived {
            value["archivedAt"] = json!(archived);
        }
        let metadata: WritableSessionMetadata = from_value(&JsValue::from(value.clone())).unwrap();
        write_session_metadata(&native_db, &metadata).unwrap();
        steps.push(json!({"op":"call","args":["writeSessionMetadata",[source_db,value]]}));
    }
    steps.push(json!({"op":"archiveCandidates","args":[source_db,"2020-03-01T00:00:00.000Z"]}));
    let response = source.request(&json!({"id":1,"steps":steps}));
    assert_eq!(response["ok"], true, "{response}");
    let expected = &response["output"].as_array().unwrap().last().unwrap()["value"];
    let actual = archive_sessions_older_than(&native_db, "2020-03-01T00:00:00.000Z").unwrap();
    assert_eq!(JsValue::from(json!(actual)), JsValue::from(expected.clone()));
    assert_eq!(
        list_session_metadata(&native_db)
            .unwrap()
            .into_iter()
            .filter(|row| row.is_archived == Some(true))
            .count(),
        1
    );
}
