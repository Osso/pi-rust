use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::list_runtime_mailbox_messages;
use pi_js_compat::js_json::JsString;
use serde_json::json;

#[test]
fn fresh_database_reads_do_not_wait_for_an_unrelated_writer() {
    let root = tempfile::tempdir().unwrap();
    let path = JsString::from(root.path().join("control.sqlite").to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let request = |id| json!({"id":id,"steps":[{"op":"call","args":["listRuntimeMailboxMessages",[path]]}]});
    let initialized = source.request(&request(1));
    assert_eq!(initialized["output"][0]["ok"], true, "{initialized}");
    let writer = rusqlite::Connection::open(root.path().join("control.sqlite")).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let expected = source.request(&request(2));
    assert_eq!(expected["output"][0]["ok"], true, "{expected}");
    assert_eq!(expected["output"][0]["value"], json!([]));
    assert!(
        list_runtime_mailbox_messages(&path)
            .expect("read-only operation must not acquire the writer lock for an empty retention sweep")
            .is_empty()
    );
    writer.execute_batch("ROLLBACK").unwrap();
}
