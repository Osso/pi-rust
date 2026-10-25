use pi_js_compat::js_json::JsString;
#[path = "support/repin_fixture_process.rs"]
mod fixture_process;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::{js_json, js_json as json};
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], json!(true), "{r:?}");
    r["output"][0]["value"].clone()
}
fn metadata(path: &str, id: &str, count: i64, child: bool) -> Value {
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    json!({"sessionPath":path,"id":id,"cwd":"/fixture","createdAt":now,"modifiedAt":now,"messageCount":count,"firstMessage":"fixture","isSubagent":child})
}
#[test]
fn resume_and_empty_candidate_queries_match_real_source_for_both_init_orders() {
    for ts_first in [true, false] {
        let t = tempfile::tempdir().unwrap();
        let db = t
            .path()
            .join("control.sqlite")
            .to_str()
            .expect("utf-8 test path")
            .to_owned();
        let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
        if ts_first {
            assert!(call(&mut ts, "listResumeSessionMetadata", json!([db,{"archived":false}])).is_null())
        } else {
            assert!(
                list_resume_session_metadata(
                    &db,
                    &ResumeSessionMetadataOptions {
                        archived: false,
                        cwd: None,
                        session_path_prefix: None
                    }
                )
                .unwrap()
                .is_none()
            )
        }
        for (path, id, count, child) in [
            ("/fixture/%_literal/a", "a", 0, false),
            ("/fixture/other/b", "b", 1, false),
            ("/fixture/%_literal/child", "c", 0, true),
        ] {
            call(
                &mut ts,
                "writeSessionMetadata",
                json!([db, metadata(path, id, count, child)]),
            );
        }
        for archived in [false, true] {
            let options = ResumeSessionMetadataOptions {
                archived,
                cwd: Some("/fixture".into()),
                session_path_prefix: Some("/fixture/%_literal/".into()),
            };
            assert_eq!(
                js_json::to_value(&list_resume_session_metadata(&db, &options).unwrap()).unwrap(),
                call(&mut ts, "listResumeSessionMetadata", json!([db, options]))
            );
        }
        assert_eq!(
            js_json::to_value(&list_empty_main_session_candidates(&db).unwrap()).unwrap(),
            call(&mut ts, "listEmptyMainSessionCandidates", json!([db]))
        );
        call(&mut ts, "archiveSession", json!([db, "/fixture/%_literal/a"]));
        assert!(list_empty_main_session_candidates(&db).unwrap().is_empty());
    }
}
#[test]
fn restriction_claim_and_known_local_admission_share_peer_state() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut ts, "claimCodexRestrictionRecovery", json!([db, "peer"])),
        json!(true)
    );
    assert!(!claim_codex_restriction_recovery(&db, &JsString::from("peer")).unwrap());
    assert!(claim_codex_restriction_recovery(&db, &JsString::from("rust")).unwrap());
    assert_eq!(
        call(&mut ts, "claimCodexRestrictionRecovery", json!([db, "rust"])),
        json!(false)
    );
    assert!(!is_known_local_session(&db, "known").unwrap());
    call(
        &mut ts,
        "writeSessionMetadata",
        json!([db, metadata("/fixture/s", "known", 1, false)]),
    );
    assert!(is_known_local_session(&db, "known").unwrap());
    assert_eq!(call(&mut ts, "isKnownLocalSession", json!([db, "known"])), json!(true));
}
#[test]
fn live_agent_ids_track_owned_peer_process_lifetime() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let response = ts
        .request(&json!({"id":1,"steps":[{"op":"registerSelf","args":[db,{"sessionId":"s","agentId":"child"},null]}]}));
    assert_eq!(response["output"][0]["ok"], json!(true));
    assert_eq!(
        read_live_runtime_mailbox_session_agent_ids(&db, "s").unwrap(),
        vec![Some("child".into())]
    );
    let identity =
        js_json::from_value(&js_json::parse_lossless(response["output"][0]["value"].as_string().unwrap()).unwrap())
            .unwrap();
    drop(ts);
    fixture_process::wait_for_owned_fixture_exit(&identity);
    assert!(
        read_live_runtime_mailbox_session_agent_ids(&db, "s")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn transaction_enqueue_rolls_back_collision_and_publishes_only_committed_rows() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    call(&mut ts, "listRuntimeMailboxMessages", json!([db]));
    let value = json!({"recipient":{"sessionId":"to","agentId":null},"sender":{"sessionId":"from","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/s","messageId":"m"},"message":{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"original"}});
    let input: EnqueueStoredRuntimeMailboxMessageInput = js_json::from_value(&value.clone()).unwrap();
    let mut collision = value;
    collision["message"]["body"] = json!("collision");
    let collision: EnqueueStoredRuntimeMailboxMessageInput = js_json::from_value(&collision).unwrap();
    assert!(
        with_control_db(&JsString::from(&db), |db| with_immediate_transaction(db, |db| {
            enqueue_stored_runtime_mailbox_message_in_transaction(db, &input)?;
            enqueue_stored_runtime_mailbox_message_in_transaction(db, &collision)
        }))
        .is_err()
    );
    assert!(
        call(&mut ts, "listRuntimeMailboxMessages", json!([db]))
            .as_array()
            .unwrap()
            .is_empty()
    );
    let id = with_control_db(&JsString::from(&db), |db| {
        with_immediate_transaction(db, |db| {
            enqueue_stored_runtime_mailbox_message_in_transaction(db, &input)
        })
    })
    .unwrap();
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["body"],
        json!("original")
    );
    notify_runtime_mailbox_recipient(&db, &input.recipient).unwrap();
}

#[test]
fn legacy_live_identity_without_incarnation_is_not_superseded_by_current_nonce() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let identity = pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).unwrap();
    register_runtime_mailbox_listener(
        &db,
        &RuntimeMailboxAddress {
            session_id: "legacy".into(),
            agent_id: None,
        },
        std::process::id(),
        None,
        RuntimeMailboxRegistrationOptions {
            runtime_instance_id: Some(serde_json::to_string(&identity).unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let peer = call(&mut ts, "readLiveRuntimeMailboxSessionAgentIds", json!([db, "legacy"]));
    assert_eq!(peer, json!([null]));
    assert_eq!(
        js_json::to_value(&read_live_runtime_mailbox_session_agent_ids(&db, "legacy").unwrap()).unwrap(),
        peer
    );
}
