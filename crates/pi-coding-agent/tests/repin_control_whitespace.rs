use pi_js_compat::js_json::JsString;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::multi_agent_store::*;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::{js_json, js_json as json};
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], json!(true), "{r:?}");
    r["output"][0]["value"].clone()
}
#[test]
fn architect_post_matches_js_trim_and_claim_tokens() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let input = PostArchitectRequestInput {
        sender_session_id: "sender".into(),
        project_cwd: "/fixture".into(),
        body: "\u{feff} request \u{a0}".into(),
    };
    let peer = call(&mut ts, "postArchitectRequest", json!([db, input]))
        .as_f64()
        .unwrap() as i64;
    let native = post_architect_request(&db, &input).unwrap();
    let rows = call(&mut ts, "listPendingArchitectRequests", json!([db, 20]));
    let rows = rows.as_array().unwrap();
    assert_eq!(
        rows.iter().find(|r| r["id"] == json!(peer)).unwrap()["body"],
        rows.iter().find(|r| r["id"] == json!(native)).unwrap()["body"]
    );
    let claimed = claim_pending_architect_requests(&db, &JsString::from("claim"), 20).unwrap();
    assert_eq!(claimed.len(), 2);
    assert!(renew_architect_request_claims(&db, &[peer, native], &JsString::from("wrong")).is_err());
    renew_architect_request_claims(&db, &[peer, native], &JsString::from("claim")).unwrap();
    complete_architect_request(&db, peer, &JsString::from("claim"), Some(&JsString::from("sender"))).unwrap();
    assert_eq!(
        call(&mut ts, "claimPendingArchitectRequests", json!([db, "next", 20])),
        json!([])
    );
}
#[test]
fn prompt_history_trims_js_whitespace_before_adjacent_dedup() {
    let t = tempfile::tempdir().unwrap();
    let ts_db = t.path().join("ts.sqlite").to_str().expect("utf-8 test path").to_owned();
    let rust_db = t
        .path()
        .join("rust.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    for value in ["\u{feff}same", "same", "\u{0085}"] {
        call(&mut ts, "recordPromptHistoryEntry", json!([ts_db, value]));
        record_prompt_history_entry(&rust_db, value).unwrap();
    }
    assert_eq!(
        js_json::to_value(&read_prompt_history(&rust_db, 20).unwrap()).unwrap(),
        call(&mut ts, "readPromptHistory", json!([ts_db, 20]))
    );
}
#[test]
fn legacy_prompt_history_preserves_source_return_and_trimmed_storage() {
    let t = tempfile::tempdir().unwrap();
    let ts_db = t.path().join("ts.sqlite").to_str().expect("utf-8 test path").to_owned();
    let rust_db = t
        .path()
        .join("rust.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let entries: Vec<JsString> = vec!["\u{feff}same".into(), "same".into(), "\u{0085}".into()];
    let expected = call(&mut ts, "readOrMigratePromptHistory", json!([ts_db, entries, 20]));
    assert_eq!(
        js_json::to_value(&read_or_migrate_prompt_history(&rust_db, &entries, 20).unwrap()).unwrap(),
        expected
    );
    assert_eq!(
        js_json::to_value(&read_prompt_history(&rust_db, 20).unwrap()).unwrap(),
        call(&mut ts, "readPromptHistory", json!([ts_db, 20]))
    );
}
#[test]
fn terminal_summary_and_error_text_use_js_trim() {
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let mut ts = oracle::TsOracle::spawn(
        "multi-agent-store-oracle.ts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    for lifecycle in ["completed", "failed"] {
        let mut snapshot = json!({"id":"child","parentId":"main","displayName":"Worker","agentType":"worker","lifecycle":lifecycle,"revision":2,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}});
        if lifecycle == "completed" {
            snapshot["result"] = json!({"summary":"\u{feff} summary \u{a0}"})
        } else {
            snapshot["error"] = json!({"message":"\u{feff} failure \u{a0}"})
        };
        let clock = now.clone();
        let store = MultiAgentStore::new(MultiAgentStoreOptions {
            now: Some(std::sync::Arc::new(move || Ok(clock.clone().into()))),
        });
        store
            .publish_terminal_outbox_snapshot(&js_json::from_value(&snapshot.clone()).unwrap())
            .unwrap();
        let actual = js_json::to_value(&store.list_mailbox_messages().unwrap()).unwrap();
        let peer=ts.request(&json!({"id":1,"now":now,"steps":[{"op":"publishTerminalOutboxSnapshot","args":[snapshot]},{"op":"listMailboxMessages","args":[]}]}));
        assert_eq!(actual, peer["output"][1]["value"], "{lifecycle}");
    }
}
