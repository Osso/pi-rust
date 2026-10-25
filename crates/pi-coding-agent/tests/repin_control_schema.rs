#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::read_session_metadata;
use serde_json::json;
#[test]
fn rust_schema_provides_frozen_birth_origin_and_recovery_contracts_before_ts_initializes() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("control.sqlite").display().to_string();
    read_session_metadata(&db, "/absent").unwrap();
    let raw = rusqlite::Connection::open(&db).unwrap();
    (|db: &rusqlite::Connection| -> Result<(),rusqlite::Error> {
        db.execute("INSERT INTO multi_agent_mailbox_messages(session_path,message_id,data,updated_at) VALUES('/fixture/s','m','{\"id\":\"m\"}','now')", [])?;
        let birth: Option<String> = db.query_row("SELECT json_extract(data,'$.createdAt') FROM multi_agent_mailbox_messages",[],|r|r.get(0))?;
        assert!(birth.is_some(),"frozen INSERT trigger must assign birth before TS opens");
        db.execute("INSERT INTO shared_channel_messages(sender_session_id,body,created_at,origin_host,origin_id) VALUES('s','b','now','peer',1)",[])?;
        assert!(db.execute("INSERT INTO shared_channel_messages(sender_session_id,body,created_at,origin_host,origin_id) VALUES('s','b','now','peer',1)",[]).is_err());
        db.execute("INSERT INTO codex_restriction_recovery_claims VALUES('request','now')",[])?;
        assert!(db.execute("INSERT INTO codex_restriction_recovery_claims VALUES('request','now')",[]).is_err());
        Ok(())
    })(&raw).unwrap();
    drop(raw);
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let response = ts.request(&json!({"id":1,"steps":[{"op":"call","args":["readSessionMetadata",[db,"/absent"]]}]}));
    assert_eq!(response["output"][0]["ok"], true, "{response}");
}
