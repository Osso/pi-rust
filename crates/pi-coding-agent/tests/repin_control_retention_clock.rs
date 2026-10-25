//! Run the actual native retention module against source-created SQLite with an
//! explicit relative clock, without adding production runtime clock flags.
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::ControlDbError;
use pi_coding_agent::utils;
use pi_js_compat::js_json::JsString;
#[path = "../../pi-coding-agent-storage/src/core/session_control_db/retention.rs"]
mod retention_contract;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;
fn step(ts: &mut oracle::TsOracle, op: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":op,"args":args}]}));
    assert_eq!(r["output"][0]["ok"], json!(true), "{r:?}");
    r["output"][0]["value"].clone()
}
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    step(ts, "call", json!([name, args]))
}
#[test]
fn exact_creation_boundary_and_invalid_birth_match_frozen_sqlite_cleanup() {
    let t = tempfile::tempdir().unwrap();
    let rust = t
        .path()
        .join("rust.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let reference = t
        .path()
        .join("reference.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let base = pi_js_compat::date::js_now_millis() as i64;
    let boundary = utils::iso_time::format_iso_millis(base);
    let fresh = utils::iso_time::format_iso_millis(base + 1);
    let now = base + retention_contract::MAILBOX_MESSAGE_RETENTION_MS;
    for db in [&rust, &reference] {
        for id in ["boundary", "fresh", "missing", "null", "number", "invalid"] {
            let birth = if id == "fresh" { &fresh } else { &boundary };
            call(
                &mut ts,
                "enqueueStoredRuntimeMailboxMessage",
                json!([db,{"recipient":{"sessionId":"to","agentId":null},"sender":{"sessionId":"from","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/s","messageId":id},"message":{"id":id,"fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","createdAt":birth,"body":id}}]),
            );
        }
        for body in ["boundary", "fresh"] {
            call(
                &mut ts,
                "postSharedChannelMessage",
                json!([db,{"sender":{"sessionId":"fixture","agentId":null},"body":body}]),
            );
        }
        step(
            &mut ts,
            "fixtureExec",
            json!([
                db,
                format!(
                    "UPDATE multi_agent_mailbox_messages SET data=json_remove(data,'$.createdAt') WHERE message_id='missing'; UPDATE multi_agent_mailbox_messages SET data=json_set(data,'$.createdAt',NULL) WHERE message_id='null'; UPDATE multi_agent_mailbox_messages SET data=json_set(data,'$.createdAt',42) WHERE message_id='number'; UPDATE multi_agent_mailbox_messages SET data=json_set(data,'$.createdAt','not-a-date') WHERE message_id='invalid'; UPDATE shared_channel_messages SET created_at=CASE body WHEN 'fresh' THEN '{fresh}' ELSE '{boundary}' END;"
                )
            ]),
        );
    }
    let db = rusqlite::Connection::open(&rust).unwrap();
    retention_contract::cleanup_expired_messages_at(&db, now).unwrap();
    let rust_ids: Vec<String> = db
        .prepare("SELECT message_id FROM multi_agent_mailbox_messages ORDER BY rowid")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rust_ids, vec!["fresh"]);
    let channel: Vec<String> = db
        .prepare("SELECT body FROM shared_channel_messages ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(channel, vec!["fresh"]);
    let expected = step(&mut ts, "retentionAt", json!([reference, now]));
    assert_eq!(
        expected
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["storeRef"]["messageId"].as_string().unwrap())
            .collect::<Vec<_>>(),
        rust_ids.iter().map(String::as_str).collect::<Vec<_>>()
    );
    let peer_reads_rust = step(&mut ts, "retentionAt", json!([rust, now]));
    assert_eq!(peer_reads_rust[0]["storeRef"]["messageId"], json!("fresh"));
    assert_eq!(
        call(&mut ts, "listSharedChannelMessagesAfter", json!([rust, 0, 20]))[0]["body"],
        json!("fresh")
    );
    assert!(retention_contract::is_mailbox_message_expired(
        Some(&JsString::from(&boundary)),
        now as f64
    ));
    assert!(!retention_contract::is_mailbox_message_expired(
        Some(&JsString::from(&fresh)),
        now as f64
    ));
    assert!(retention_contract::is_mailbox_message_expired(None, now as f64));
    assert!(retention_contract::is_mailbox_message_expired(
        Some(&JsString::from("invalid")),
        now as f64
    ));
    let fresh_payload = json!({"createdAt":fresh});
    assert!(
        retention_contract::assert_mailbox_message_fresh(
            fresh_payload.as_object().unwrap(),
            &JsString::from("fixture")
        )
        .is_ok()
    );
    let old_payload = json!({"createdAt":utils::iso_time::format_iso_millis(base-2*86_400_000)});
    assert!(
        retention_contract::assert_mailbox_message_fresh(old_payload.as_object().unwrap(), &JsString::from("fixture"))
            .is_err()
    );
    retention_contract::cleanup_expired_messages(&db).unwrap();
    let before = pi_js_compat::date::js_now_millis() as i64;
    let cutoff = retention_contract::mailbox_cutoff_ms();
    let after = pi_js_compat::date::js_now_millis() as i64;
    assert!((before - 86_400_000..=after - 86_400_000).contains(&cutoff));
}
