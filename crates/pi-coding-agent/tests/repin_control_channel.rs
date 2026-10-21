#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use serde_json::{Value, json};
fn step(ts: &mut oracle::TsOracle, op: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":op,"args":args}]}));
    assert_eq!(r["output"][0]["ok"], true, "{r}");
    r["output"][0]["value"].clone()
}
#[test]
fn foreign_origin_same_session_is_pending_and_preserved_across_peers() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let recipient = RuntimeMailboxAddress {
        session_id: "same".into(),
        agent_id: None,
    };
    step(
        &mut ts,
        "call",
        json!(["initializeSharedChannelCursorAtTail", [db, recipient]]),
    );
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    step(
        &mut ts,
        "fixtureSql",
        json!([
            db,
            "INSERT INTO shared_channel_messages(sender_session_id,body,created_at,origin_host,origin_id) VALUES(?,?,?,?,?) RETURNING id",
            ["same", "foreign", now, "fixture-foreign-host", 7]
        ]),
    );
    assert!(
        has_pending_runtime_coordination_message(&db, &recipient).unwrap(),
        "remote origin is not self"
    );
    let rust = list_shared_channel_messages_after(&db, 0, 20, i64::MAX).unwrap();
    let peer = step(&mut ts, "call", json!(["listSharedChannelMessagesAfter", [db, 0, 20]]));
    assert_eq!(serde_json::to_value(&rust).unwrap(), peer);
    advance_shared_channel_cursor(&db, &recipient, rust[0].id).unwrap();
    advance_shared_channel_cursor(&db, &recipient, 0).unwrap();
    assert_eq!(
        step(&mut ts, "call", json!(["readSharedChannelCursor", [db, recipient]])),
        json!(rust[0].id)
    );
}
#[test]
fn initialization_expires_old_channel_rows_created_by_ts() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let id = step(
        &mut ts,
        "call",
        json!(["postSharedChannelMessage",[db,{"sender":{"sessionId":"s","agentId":null},"body":"old"}]]),
    );
    let old = pi_coding_agent::utils::iso_time::format_iso_millis(
        pi_js_compat::date::js_now_millis() as i64 - 2 * 86_400_000,
    );
    step(
        &mut ts,
        "fixtureSql",
        json!([
            db,
            "UPDATE shared_channel_messages SET created_at = ? WHERE id = ? RETURNING id",
            [old, id]
        ]),
    );
    assert!(
        list_shared_channel_messages_after(&db, 0, 20, i64::MAX)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn retained_connection_sweeps_channel_on_source_minute_timer() {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let id = step(
        &mut ts,
        "call",
        json!(["postSharedChannelMessage",[db,{"sender":{"sessionId":"s","agentId":null},"body":"retained"}]]),
    );
    let retained = retain_control_db_connection(&db).unwrap();
    let old = pi_coding_agent::utils::iso_time::format_iso_millis(
        pi_js_compat::date::js_now_millis() as i64 - 2 * 86_400_000,
    );
    step(
        &mut ts,
        "fixtureSql",
        json!([
            db,
            "UPDATE shared_channel_messages SET created_at=? WHERE id=? RETURNING id",
            [old, id]
        ]),
    );
    assert_eq!(
        list_shared_channel_messages_after(&db, 0, 20, i64::MAX).unwrap().len(),
        1,
        "source list relies on timer, not per-read deletion"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(64);
    loop {
        if list_shared_channel_messages_after(&db, 0, 20, i64::MAX)
            .unwrap()
            .is_empty()
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "retained DB must sweep without reopen"
        );
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    drop(retained);
    assert!(
        step(&mut ts, "call", json!(["listSharedChannelMessagesAfter", [db, 0, 20]]))
            .as_array()
            .unwrap()
            .is_empty()
    );
}
