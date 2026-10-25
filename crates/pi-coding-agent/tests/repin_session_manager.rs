#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::AgentMessage;
use pi_coding_agent::core::session_manager::{SessionManager, SessionManagerError, SetMetadataControlDbPathOptions};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::fs;

fn source() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "session-manager-repin-oracle.mts",
        "packages/coding-agent/src/core/session-manager.ts",
    )
}

fn call(ts: &mut oracle::TsOracle, request: Value) -> Value {
    let response = ts.request(&request);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}

fn native_error(error: SessionManagerError) -> Value {
    match error.native_error() {
        Some(native) => native.value().to_serde().unwrap(),
        None => json!({"name":error.name(), "message":error.message()}),
    }
}

fn user(text: &str) -> AgentMessage {
    serde_json::from_value(json!({"role":"user","content":text,"timestamp":1})).unwrap()
}

fn created(ts: &mut oracle::TsOracle, dir: &str, id: &str) -> String {
    let out = call(
        ts,
        json!({"id":1,"mode":"create","cwd":dir,"dir":dir,"sessionId":id,
        "commands":[{"op":"user","text":"original"},{"op":"persist"}]}),
    );
    let header = &out["state"]["header"];
    let timestamp = header["timestamp"].as_str().unwrap().replace([':', '.'], "-");
    format!("{dir}/{timestamp}_{id}.jsonl")
}

#[test]
fn append_recreates_removed_flushed_transcript_like_frozen() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let path = created(&mut ts, dir, "removed");
    let mut manager = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    let peer = call(
        &mut ts,
        json!({"id":2,"mode":"open","path":path,
        "commands":[{"op":"remove"},{"op":"user","text":"after removal"}]}),
    );
    fs::remove_file(&path).unwrap();
    manager.append_message(user("after removal")).unwrap();
    let records: Vec<Value> = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let expected: Vec<Value> = peer["state"]["bytes"]
        .as_str()
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        records.len(),
        1,
        "appendFileSync creates a new file, not a replacement header"
    );
    assert_eq!(records[0]["message"], expected[0]["message"]);
    assert_eq!(records[0]["parentId"], expected[0]["parentId"]);
    assert_eq!(manager.get_entries().len(), 2);
}

#[test]
fn empty_control_db_skips_sandbox_persistence_like_frozen() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let expected = call(
        &mut ts,
        json!({"id":1,"mode":"create","cwd":dir,"dir":dir,"sessionId":"empty-db",
        "commands":[{"op":"db","path":""},{"op":"sandbox"}]}),
    );
    let mut manager = SessionManager::create(&dir.into(), Some(&dir.into()), None).unwrap();
    manager
        .set_metadata_control_db_path(Some(&JsString::default()), SetMetadataControlDbPathOptions::default())
        .unwrap();
    let actual = manager.set_session_sandbox_profile(None).unwrap();
    assert_eq!(json!(actual), expected["results"][1]);
    assert!(!std::path::Path::new(manager.get_session_file().unwrap().to_utf8().unwrap().as_str()).exists());
}

#[test]
fn in_memory_file_switch_rejects_archives_like_frozen() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let path = format!("{dir}/missing.jsonl.zst");
    let mut ts = source();
    let peer = ts.request(&json!({"id":1,"mode":"memory","cwd":dir,"commands":[{"op":"switch","path":path}]}));
    assert_eq!(peer["ok"], false);
    let mut manager = SessionManager::in_memory(Some(&dir.into()), None).unwrap();
    let actual = manager.set_session_file(&path.as_str().into()).unwrap_err();
    assert_eq!(native_error(actual), peer["error"]);
    assert_eq!(manager.get_session_file(), Some(&JsString::from(path)));
}

#[test]
fn filesystem_failures_keep_frozen_error_records_and_append_state() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let path = created(&mut ts, dir, "errors");
    let mut manager = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    let peer = ts.request(&json!({"id":2,"mode":"open","path":path,
        "commands":[{"op":"directory"},{"op":"user","text":"failed append"}]}));
    assert_eq!(peer["ok"], false);
    let actual = manager.append_message(user("failed append")).unwrap_err();
    assert_eq!(native_error(actual), peer["error"]);
    assert_eq!(
        manager.get_entries().len(),
        peer["state"]["entries"].as_array().unwrap().len()
    );
    assert_eq!(
        manager.get_leaf_entry().unwrap().message().unwrap().as_ref(),
        &user("failed append")
    );

    let parent_file = format!("{dir}/not-a-directory");
    fs::write(&parent_file, "file").unwrap();
    let blocked_dir = format!("{parent_file}/sessions");
    let peer = ts.request(&json!({"id":3,"mode":"create","cwd":dir,"dir":blocked_dir}));
    let actual = SessionManager::create(&dir.into(), Some(&blocked_dir.into()), None)
        .err()
        .unwrap();
    assert_eq!(native_error(actual), peer["error"]);

    let peer = ts.request(&json!({"id":4,"mode":"load","path":dir}));
    let actual = pi_coding_agent::core::session_manager::load_entries_from_file(&dir.into()).unwrap_err();
    assert_eq!(native_error(actual), peer["error"]);
}
