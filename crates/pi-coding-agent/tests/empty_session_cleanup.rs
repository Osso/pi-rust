use crate::shared_oracle as oracle;

use pi_coding_agent::core::empty_session_cleanup::{remove_abandoned_empty_session, sweep_abandoned_empty_sessions};
use pi_coding_agent::core::session_control_db::{
    WritableSessionMetadata, list_session_metadata, read_session_metadata, write_session_health, write_session_metadata,
};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Clone, Copy)]
struct RemoveCase {
    name: &'static str,
    id: &'static str,
    file: Option<&'static str>,
    db: bool,
    same_target: bool,
    message: bool,
    missing: bool,
}

fn initialize_remove(root: &Path, case: RemoveCase) -> (SessionManager, Option<JsString>, Option<JsString>) {
    let file = case.file.map(|file| root.join(file));
    let mut manager = match &file {
        Some(file) => {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            let header =
                json!({"type":"session","version":3,"id":case.id,"cwd":root,"timestamp":"2026-10-23T00:00:00.000Z"});
            let mut lines = format!("{header}\n");
            if case.message {
                lines.push_str(&format!("{}\n", json!({"type":"message","id":"m1","parentId":null,"timestamp":"2026-10-23T00:00:00.000Z","message":{"role":"user","content":"keep","timestamp":1}})));
            }
            std::fs::write(file, lines).unwrap();
            SessionManager::open(
                &file.to_str().unwrap().into(),
                Some(&root.to_str().unwrap().into()),
                None,
            )
            .unwrap()
        }
        None => SessionManager::in_memory(Some(&root.to_str().unwrap().into()), None).unwrap(),
    };
    let db = case
        .db
        .then(|| JsString::from(root.join("control.db").to_str().unwrap()));
    manager
        .set_metadata_control_db_path(db.as_ref(), Default::default())
        .unwrap();
    if case.missing {
        std::fs::remove_file(file.as_ref().unwrap()).unwrap();
    }
    (manager, file.map(|path| path.to_str().unwrap().into()), db)
}

#[test]
fn remove_guards_and_filesystem_effects_match_the_pinned_module() {
    let base = RemoveCase {
        name: "plain",
        id: "fixture",
        file: Some("session.jsonl"),
        db: true,
        same_target: false,
        message: false,
        missing: false,
    };
    let cases = [
        base,
        RemoveCase {
            name: "no-db",
            db: false,
            ..base
        },
        RemoveCase {
            name: "no-file",
            file: None,
            ..base
        },
        RemoveCase {
            name: "same-resolved-target",
            same_target: true,
            ..base
        },
        RemoveCase {
            name: "supervisor-id",
            id: "supervisor",
            ..base
        },
        RemoveCase {
            name: "architect-id",
            id: "architect",
            ..base
        },
        RemoveCase {
            name: "resident-slash",
            file: Some("architect-sessions/session.jsonl"),
            ..base
        },
        RemoveCase {
            name: "resident-backslash",
            file: Some("x\\supervisor-sessions\\session.jsonl"),
            ..base
        },
        RemoveCase {
            name: "message",
            message: true,
            ..base
        },
        RemoveCase {
            name: "missing-file",
            missing: true,
            ..base
        },
    ];
    let temp = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn(
        "empty-session-cleanup-functions-oracle.mts",
        "packages/coding-agent/src",
    );
    for case in cases {
        let source_root = temp.path().join(format!("source-{}", case.name));
        let native_root = temp.path().join(format!("native-{}", case.name));
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::create_dir_all(&native_root).unwrap();
        let source_file = case.file.map(|file| source_root.join(file));
        let source_target = case.same_target.then(|| source_root.join("./session.jsonl"));
        let expected = source.request(&json!({"operation":"remove","cwd":source_root,"db":case.db.then(||source_root.join("control.db")),"file":source_file,"target":source_target,"id":case.id,"entries":if case.message {vec![json!({"type":"message"})]}else{Vec::new()},"existing":!case.missing,"rows":[]}));
        assert_eq!(expected["ok"], true, "case {}: {expected}", case.name);
        let (manager, file, db) = initialize_remove(&native_root, case);
        let target = case
            .same_target
            .then(|| JsString::from(native_root.join("./session.jsonl").to_str().unwrap()));
        remove_abandoned_empty_session(&manager, target.as_ref()).unwrap();
        let actual = json!({"ok":true,"fileExists":file.as_ref().is_some_and(|file|Path::new(&encode_node_utf8(file)).exists()),"metadataExists":match (db.as_ref(),file.as_ref()){(Some(db),Some(file))=>read_session_metadata(db,file).unwrap().is_some(),_=>false}});
        assert_eq!(actual, expected, "case {}", case.name);
    }
}

fn sweep_rows(root: &Path) -> Vec<Value> {
    [
        json!({"id":"absent"}),
        json!({"id":"dead","pid":"dead"}),
        json!({"id":"zero","pid":"zero"}),
        json!({"id":"live","pid":"self"}),
        json!({"id":"existing","existing":true}),
        json!({"id":"supervisor"}),
        json!({"id":"resident-path","relative":"architect-sessions/session.jsonl"}),
        json!({"id":"has-message","messageCount":1}),
        json!({"id":"archived","archivedAt":"2026-10-22T00:00:00.000Z"}),
        json!({"id":"subagent","isSubagent":true}),
    ]
    .into_iter()
    .map(|mut row| {
        let relative = row
            .get("relative")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}.jsonl", row["id"].as_str().unwrap()));
        row["sessionPath"] = json!(root.join(relative));
        row["cwd"] = json!(root);
        row["createdAt"] = json!("2026-10-23T00:00:00.000Z");
        row["modifiedAt"] = json!("2026-10-23T00:00:00.000Z");
        row["firstMessage"] = json!("");
        if row.get("messageCount").is_none() {
            row["messageCount"] = json!(0);
        }
        row
    })
    .collect()
}

#[test]
fn sweep_selects_only_missing_empty_main_nonresident_sessions_without_live_pi_health() {
    let temp = tempfile::tempdir().unwrap();
    let source_root = temp.path().join("source");
    let native_root = temp.path().join("native");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&native_root).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "empty-session-cleanup-functions-oracle.mts",
        "packages/coding-agent/src",
    );
    let expected = source
        .request(&json!({"operation":"sweep","db":source_root.join("control.db"),"rows":sweep_rows(&source_root)}));
    assert_eq!(expected["ok"], true, "source {expected}");
    let db = JsString::from(native_root.join("control.db").to_str().unwrap());
    for row in sweep_rows(&native_root) {
        let metadata: WritableSessionMetadata = serde_json::from_value(row.clone()).unwrap();
        write_session_metadata(&db, &metadata).unwrap();
        if row.get("existing") == Some(&Value::Bool(true)) {
            std::fs::write(encode_node_utf8(&metadata.session_path), "fixture").unwrap();
        }
        if let Some(pid) = row.get("pid").and_then(Value::as_str) {
            let pid = match pid {
                "self" => std::process::id(),
                "zero" => 0,
                "dead" => 2147483647,
                _ => unreachable!(),
            };
            let health = serde_json::from_value(json!({"sessionId":metadata.id,"agentGeneration":1,"pid":pid,"lastActiveAt":null,"lastCheckedAt":null,"checkStatus":"ok","checkedGeneration":null,"checkLatencyMs":null,"updatedAt":"2026-10-23T00:00:00.000Z"})).unwrap();
            write_session_health(&db, &health).unwrap();
        }
    }
    let count = sweep_abandoned_empty_sessions(&db).unwrap();
    let mut remaining: Vec<_> = list_session_metadata(&db)
        .unwrap()
        .into_iter()
        .map(|row| row.id)
        .collect();
    remaining.sort();
    assert_eq!(json!({"ok":true,"count":count,"remaining":remaining}), expected);
    assert_eq!(count, 3);
}
