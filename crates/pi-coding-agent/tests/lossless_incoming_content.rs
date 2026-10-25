//! Native incoming content survives DTO ingress; SQLite applies the frozen Node bind semantics.
#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_control_db::{
    IncomingControlMessage, claim_latest_incoming_message, complete_incoming_message, enqueue_incoming_message,
    fail_incoming_message, read_incoming_message_status,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString};

#[test]
fn incoming_content_remains_native_until_the_sqlite_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let rust_path = directory.path().join("rust.sqlite");
    let source_path = directory.path().join("source.sqlite");
    let rust_db = JsString::from(rust_path.to_str().expect("utf-8 test path"));
    let source_db = JsString::from(source_path.to_str().expect("utf-8 test path"));
    let content = JsString::from_utf16(vec![0x61, 0xd800, 0x62, 0xdfff, 0xd83d, 0xde80, 0]);
    let original = content.clone();
    let message: IncomingControlMessage = js_json::from_value(&json!({"id": 1, "content": content})).unwrap();
    assert_eq!(js_json::to_value(&message).unwrap()["content"], json!(content));

    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let response = source.request(&json!({"id":1,"steps":[
        {"op":"call","args":["enqueueIncomingMessage",[source_db,content]]},
        {"op":"call","args":["claimLatestIncomingMessage",[source_db]]}
    ]}));
    assert_eq!(response["output"][0]["ok"], json!(true), "{response:?}");
    assert_eq!(response["output"][1]["ok"], json!(true), "{response:?}");
    let expected = &response["output"][1]["value"];
    let id = enqueue_incoming_message(&rust_db, &message.content).unwrap();
    let claimed = claim_latest_incoming_message(&rust_db).unwrap().unwrap();
    assert_eq!(claimed.id, id);
    assert_eq!(js_json::to_value(&claimed).unwrap(), *expected);
    let stored: String = rusqlite::Connection::open(&rust_path)
        .unwrap()
        .query_row("SELECT content FROM incoming_messages WHERE id=?", [id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(json!(stored), expected["content"]);
    assert_eq!(content, original);
    complete_incoming_message(&rust_db, id).unwrap();
    assert_eq!(
        read_incoming_message_status(&rust_db, id).unwrap().as_deref(),
        Some("completed")
    );

    let second = enqueue_incoming_message(&rust_db, &message.content).unwrap();
    let error = JsString::from_utf16(vec![0x65, 0xd801]);
    fail_incoming_message(&rust_db, second, &error).unwrap();
    assert_eq!(
        read_incoming_message_status(&rust_db, second).unwrap().as_deref(),
        Some("failed")
    );
}
