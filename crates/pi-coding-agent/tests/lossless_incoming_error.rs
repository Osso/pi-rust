//! Native error ingress, with Node replacement only at the real SQLite TEXT bind.
use crate::shared_oracle as oracle;
use pi_coding_agent::core::session_control_db::{enqueue_incoming_message, fail_incoming_message};
use pi_js_compat::js_json::{JsString, encode_node_utf8};

#[test]
fn incoming_error_reaches_sql_binding_without_changing_native_input() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("control.sqlite");
    let db: JsString = path.to_str().unwrap().into();
    let id = enqueue_incoming_message(&db, "fixture").unwrap();
    let message = JsString::from("error-") + JsString::from_utf16(vec![0xd800, 0xdfff, 0xd801]);
    let original = message.clone();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let wire = pi_js_compat::js_json!([db, id, message]);
    let response = source.request(&serde_json::json!({"id":1,"steps":[{"op":"callWire","args":["failIncomingMessage",pi_js_compat::js_json::stringify_lossless(&wire)]}]}));
    assert_eq!(response["output"][0]["ok"], true, "{response}");
    let connection = rusqlite::Connection::open(&path).unwrap();
    let expected: String = connection
        .query_row("SELECT error FROM incoming_messages WHERE id=?", [id], |row| row.get(0))
        .unwrap();
    fail_incoming_message(&db, id, &message).unwrap();
    let actual: String = connection
        .query_row("SELECT error FROM incoming_messages WHERE id=?", [id], |row| row.get(0))
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual, encode_node_utf8(&message));
    assert_eq!(message, original);
}
