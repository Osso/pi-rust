//! Frozen TS creates real shared state; neither participant rewrites peer rows before reading.
use crate::shared_oracle as oracle;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::{js_json, js_json as json};

fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let response = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(response["output"][0]["ok"], json!(true), "{response:?}");
    response["output"][0]["value"].clone()
}
fn metadata(path: &str) -> Value {
    json!({"sessionPath":path,"id":path,"cwd":"/fixture","createdAt":"2026-01-01T00:00:00.000Z","modifiedAt":"2026-01-01T00:00:00.000Z","messageCount":2,"firstMessage":"original","allMessagesText":"obsolete"})
}
#[test]
fn metadata_survives_both_initialization_orders_and_peer_reopen() {
    for ts_first in [true, false] {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp
            .path()
            .join("control.sqlite")
            .to_str()
            .expect("utf-8 test path")
            .to_owned();
        let path = "/fixture/session.jsonl";
        let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
        if ts_first {
            call(&mut ts, "writeSessionMetadata", json!([db, metadata(path)]));
        } else {
            write_session_metadata(&db, &js_json::from_value(&metadata(path)).unwrap()).unwrap();
        }
        call(&mut ts, "readSessionMetadata", json!([db, path]));
        let row = read_session_metadata(&db, path)
            .expect("Rust reads TS initialized v15")
            .unwrap();
        assert_eq!(row.first_message, "original");
        write_session_name(&db, path, Some(&JsString::from("Rust name"))).unwrap();
        assert_eq!(
            call(&mut ts, "readSessionNameState", json!([db, path])),
            json!({"name":"Rust name","hasStoredValue":true})
        );
        call(&mut ts, "writeSessionName", json!([db, path, null]));
        assert_eq!(
            read_session_name_state(&db, path).unwrap(),
            SessionNameState {
                name: None,
                has_stored_value: true
            }
        );
        let mut update = metadata(path);
        update["firstMessage"] = json!("Rust changed");
        write_session_metadata(&db, &js_json::from_value(&update).unwrap()).unwrap();
        assert_eq!(
            call(&mut ts, "readSessionMetadata", json!([db, path]))["firstMessage"],
            json!("Rust changed")
        );
        drop(ts);
        let mut reopened = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
        assert_eq!(
            call(&mut reopened, "readSessionMetadata", json!([db, path]))["firstMessage"],
            json!("Rust changed")
        );
    }
}
#[test]
fn descendants_and_atomic_archive_relocation_match_peer() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for (path, parent) in [
        ("/fixture/root.jsonl", None),
        ("/fixture/child.jsonl.zst", Some("/fixture/root.jsonl.zst")),
        ("/fixture/grand.jsonl", Some("/fixture/child.jsonl")),
    ] {
        let mut m = metadata(path);
        m["parentSessionPath"] = json!(parent);
        m["isSubagent"] = json!(parent.is_some());
        call(&mut ts, "writeSessionMetadata", json!([db, m]));
    }
    let expected = call(
        &mut ts,
        "listDescendantSessionPaths",
        json!([db, "/fixture/root.jsonl"]),
    );
    assert_eq!(
        js_json::to_value(&list_descendant_session_paths(&db, "/fixture/root.jsonl").unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        list_subagent_session_paths(&db, "/fixture/root.jsonl.zst").unwrap(),
        vec!["/fixture/child.jsonl.zst"]
    );
    relocate_and_archive_session(&db, "/fixture/root.jsonl", "/fixture/root.jsonl.zst").unwrap();
    assert!(call(&mut ts, "readSessionMetadata", json!([db, "/fixture/root.jsonl"])).is_null());
    assert_eq!(
        call(&mut ts, "readSessionMetadata", json!([db, "/fixture/root.jsonl.zst"]))["isArchived"],
        json!(true)
    );
    relocate_and_unarchive_session(&db, "/fixture/root.jsonl.zst", "/fixture/root.jsonl").unwrap();
    assert_eq!(
        call(&mut ts, "readSessionMetadata", json!([db, "/fixture/root.jsonl"]))["isArchived"],
        json!(false)
    );
    remove_sessions_metadata(&db, &["/fixture/child.jsonl.zst".into(), "/fixture/grand.jsonl".into()]).unwrap();
    assert_eq!(
        call(
            &mut ts,
            "listDescendantSessionPaths",
            json!([db, "/fixture/root.jsonl"])
        ),
        json!([])
    );
}
#[test]
fn selecting_archive_candidates_does_not_mark_rows_archived() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    call(
        &mut ts,
        "writeSessionMetadata",
        json!([db, metadata("/fixture/s.jsonl")]),
    );
    assert_eq!(
        archive_sessions_older_than(&db, "2026-02-01T00:00:00.000Z").unwrap(),
        vec!["/fixture/s.jsonl"]
    );
    assert!(call(&mut ts, "readSessionMetadata", json!([db, "/fixture/s.jsonl"]))["archivedAt"].is_null());
}
