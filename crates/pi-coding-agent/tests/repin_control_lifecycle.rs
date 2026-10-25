#[path = "support/repin_fixture_process.rs"]
mod fixture_process;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::{js_json, js_json as json};
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], json!(true), "{r:?}");
    r["output"][0]["value"].clone()
}
fn fixture(
    suppressed: bool,
) -> (
    tempfile::TempDir,
    String,
    oracle::TsOracle,
    ProcessIdentity,
    MultiAgentRuntimeOwnerIdentity,
    String,
) {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let identity = read_process_identity(std::process::id()).unwrap();
    let owner = MultiAgentRuntimeOwnerIdentity {
        session_id: "owner".into(),
        agent_id: None,
    };
    let agent = json!({"id":"child","parentId":"main","displayName":"child","agentType":"background","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true},"detached":true,"suppressTerminalNotification":suppressed});
    assert_eq!(
        call(
            &mut ts,
            "createMultiAgentChildWithRuntimeOwnership",
            json!([db,{"sessionPath":"/fixture/s","agentId":"child","processIdentity":identity,"owner":owner,"nowIso":now,"agent":agent}])
        )["ok"],
        json!(true)
    );
    (t, db, ts, identity, owner, now)
}
fn terminal(identity: ProcessIdentity, owner: MultiAgentRuntimeOwnerIdentity, now: &str) -> FinalizeDetachedJobInput {
    FinalizeDetachedJobInput {
        session_path: "/fixture/s".into(),
        terminal: DetachedJobTerminalInput {
            job_id: "child".into(),
            owner,
            process_identity: identity,
            output_label: "Bash output".into(),
            terminal_at: now.into(),
            tool_call_id: None,
            duration_ms: Some(25),
            outcome: DetachedJobOutcome::Completed {
                exit_code: Some(0),
                summary: Some("finished".into()),
            },
            output: DetachedJobTerminalOutput {
                label: "Bash output".into(),
                path: "/fixture/output.log".into(),
                size: 2,
                sha256: "fixture-checksum".into(),
            },
        },
    }
}
#[test]
fn suppressed_detached_finalization_has_no_owner_transport() {
    let (_t, db, mut ts, identity, owner, now) = fixture(true);
    let input = terminal(identity, owner, &now);
    assert!(matches!(
        finalize_detached_job(&db, &input).unwrap(),
        FinalizeDetachedJobResult::Ok { .. }
    ));
    assert!(
        call(
            &mut ts,
            "listRuntimeMailboxMessagesForSession",
            json!([db, "/fixture/s"])
        )
        .as_array()
        .unwrap()
        .is_empty()
    );
}
#[test]
fn detached_finalization_replays_after_peer_outbox_deletion() {
    let (_t, db, mut ts, identity, owner, now) = fixture(false);
    let input = terminal(identity, owner, &now);
    let first = finalize_detached_job(&db, &input).unwrap();
    assert!(matches!(first, FinalizeDetachedJobResult::Ok { .. }));
    let record = call(&mut ts, "claimMultiAgentTerminalOutbox", json!([db, "peer", now]));
    assert_eq!(
        call(&mut ts, "deliverMultiAgentTerminalOutbox", json!([db, record])),
        json!(true)
    );
    assert_eq!(finalize_detached_job(&db, &input).unwrap(), first);
}
fn enqueue_steer(ts: &mut oracle::TsOracle, db: &str, now: &str) -> i64 {
    call(ts,"enqueueStoredRuntimeMailboxMessage",json!([db,{"recipient":{"sessionId":"child-session","agentId":"child"},"sender":{"sessionId":"owner","agentId":null},"kind":"steer","storeRef":{"sessionPath":"/fixture/s","messageId":"steer"},"message":{"id":"steer","fromAgentId":"main","toAgentId":"child","kind":"steer","status":"pending","createdAt":now,"updatedAt":now,"body":"change course"}}])).as_f64().unwrap() as i64
}
#[test]
fn direct_steering_delivery_consumes_and_returns_original_snapshot() {
    let (_t, db, mut ts, identity, owner, now) = fixture(false);
    let id = enqueue_steer(&mut ts, &db, &now);
    let result = commit_multi_agent_steering_delivery(
        &db,
        &CommitMultiAgentSteeringDeliveryInput {
            session_path: "/fixture/s".into(),
            agent_id: "child".into(),
            process_identity: identity,
            owner,
            requested_lifecycle: "running".into(),
            updated_at: now,
            message_id: "steer".into(),
        },
    )
    .unwrap();
    let CommitMultiAgentSteeringDeliveryResult::Ok { message, agent } = result else {
        panic!("steering rejected")
    };
    assert_eq!(js_json::to_value(&message.status).unwrap(), json!("pending"));
    assert_eq!(
        js_json::to_value(&&message).unwrap()["recipientSessionId"],
        json!("child-session")
    );
    assert_eq!(agent.revision, 2);
    assert!(call(&mut ts, "readRuntimeMailboxMessage", json!([db, id])).is_null());
}
#[test]
fn taking_owned_steer_claims_message_and_advances_agent_atomically() {
    let (_t, db, mut ts, _identity, _owner, now) = fixture(false);
    let id = enqueue_steer(&mut ts, &db, &now);
    let recipient = RuntimeMailboxAddress {
        session_id: "child-session".into(),
        agent_id: Some("child".into()),
    };
    let taken = take_runtime_mailbox_messages_for_delivery(&db, &recipient, |_| Ok(true), 1).unwrap();
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].status, RuntimeMailboxMessageStatus::Claimed);
    assert_eq!(
        call(&mut ts, "readMultiAgentAgent", json!([db, "/fixture/s", "child"]))["revision"],
        json!(2)
    );
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        json!("claimed")
    );
}

#[test]
fn raw_writer_owned_steering_claim_preserves_source_payload() {
    use pi_js_compat::js_json::{self, JsString, JsValue};
    let (_temporary, path, mut source, _identity, _owner, now) = fixture(false);
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"child-session","agentId":"child"}},"sender":{{"sessionId":"owner","agentId":null}},"kind":"steer","storeRef":{{"sessionPath":"/fixture/s","messageId":"steer"}},"message":{{"id":"steer","fromAgentId":"main","toAgentId":"child","kind":"steer","status":"pending","createdAt":{},"updatedAt":{},"body":"before\ud800after","unknown":{{"\ud800":"high","�":"replacement"}}}}}}]"#,
        serde_json::to_string(&path).unwrap(),
        serde_json::to_string(&now).unwrap(),
        serde_json::to_string(&now).unwrap()
    );
    let response =
        source.request(&json!({"id":1,"steps":[{"op":"callWire","args":["enqueueStoredRuntimeMailboxMessage",wire]}]}));
    assert_eq!(response["output"][0]["ok"], json!(true), "{response:?}");
    let raw_before = source.request(
        &json!({"id":1,"steps":[{"op":"fixtureSql","args":[path,"SELECT data FROM multi_agent_mailbox_messages",[]]}]}),
    );
    assert_eq!(raw_before["output"][0]["ok"], json!(true), "{raw_before:?}");
    let JsValue::Object(before) =
        js_json::parse_lossless(raw_before["output"][0]["value"][0]["data"].as_string().unwrap()).unwrap()
    else {
        panic!("canonical object")
    };
    let recipient = RuntimeMailboxAddress {
        session_id: "child-session".into(),
        agent_id: Some("child".into()),
    };
    assert_eq!(
        take_runtime_mailbox_messages_for_delivery(&path, &recipient, |_| Ok(true), 1)
            .unwrap()
            .len(),
        1
    );
    let raw_after = source.request(
        &json!({"id":1,"steps":[{"op":"fixtureSql","args":[path,"SELECT data FROM multi_agent_mailbox_messages",[]]}]}),
    );
    assert_eq!(raw_after["output"][0]["ok"], json!(true), "{raw_after:?}");
    let JsValue::Object(after) =
        js_json::parse_lossless(raw_after["output"][0]["value"][0]["data"].as_string().unwrap()).unwrap()
    else {
        panic!("canonical object")
    };
    for key in ["body", "unknown"] {
        let key = JsString::from(key);
        assert_eq!(
            after.get(&key),
            before.get(&key),
            "atomic steering claim preserves raw field {key:?}"
        );
    }
    assert_eq!(
        after.get(&JsString::from("status")),
        Some(&JsValue::String(JsString::from("claimed")))
    );
    assert_eq!(
        call(&mut source, "readMultiAgentAgent", json!([path, "/fixture/s", "child"]))["revision"],
        json!(2)
    );
}

#[test]
fn repeated_cancellation_can_add_suppression_without_second_cancel_command() {
    let (_t, db, mut ts, identity, owner, now) = fixture(false);
    assert_eq!(
        call(
            &mut ts,
            "commitMultiAgentLifecycleMutation",
            json!([db,{"sessionPath":"/fixture/s","agentId":"child","processIdentity":identity,"owner":owner,"requestedLifecycle":"cancelling","updatedAt":now}])
        )["ok"],
        json!(true)
    );
    let input = CommitMultiAgentLifecycleMutationInput {
        session_path: "/fixture/s".into(),
        agent_id: "child".into(),
        process_identity: identity,
        owner,
        requested_lifecycle: "cancelling".into(),
        updated_at: now,
        detached_cancellation: Some(DetachedCancellation {
            output_label: "Bash output".into(),
            reason: None,
            suppress_terminal_notification: Some(true),
        }),
    };
    assert!(matches!(
        commit_multi_agent_lifecycle_mutation(&db, &input).unwrap(),
        CommitMultiAgentLifecycleMutationResult::Ok { .. }
    ));
    let peer = call(&mut ts, "readMultiAgentAgent", json!([db, "/fixture/s", "child"]));
    assert_eq!(peer["suppressTerminalNotification"], json!(true));
    assert_eq!(peer["revision"], json!(3));
    assert!(
        call(&mut ts, "listRuntimeMailboxMessages", json!([db]))
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn steering_failure_after_agent_update_rolls_back_both_state_changes() {
    let (_t, db, mut ts, identity, owner, now) = fixture(false);
    let id = enqueue_steer(&mut ts, &db, &now);
    let response=ts.request(&json!({"id":1,"steps":[{"op":"fixtureSql","args":[db,"CREATE TRIGGER fixture_steering_race AFTER UPDATE ON multi_agent_agents BEGIN UPDATE multi_agent_mailbox_messages SET data=json_set(data,'$.body','raced') WHERE session_path='/fixture/s' AND message_id='steer'; END",[]]}]}));
    assert_eq!(response["output"][0]["ok"], json!(true), "{response:?}");
    let result = commit_multi_agent_steering_delivery(
        &db,
        &CommitMultiAgentSteeringDeliveryInput {
            session_path: "/fixture/s".into(),
            agent_id: "child".into(),
            process_identity: identity,
            owner,
            requested_lifecycle: "running".into(),
            updated_at: now,
            message_id: "steer".into(),
        },
    );
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("Steering message changed during delivery")
    );
    assert_eq!(
        call(&mut ts, "readMultiAgentAgent", json!([db, "/fixture/s", "child"]))["revision"],
        json!(1)
    );
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["body"],
        json!("change course")
    );
}

#[test]
fn dead_owned_fixture_runtime_recovery_honors_persisted_suppression() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut child = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let identity: ProcessIdentity = js_json::from_value(
        &js_json::parse_lossless(
            call(&mut child, "getRuntimeProcessInstanceId", json!([]))
                .as_string()
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let owner = MultiAgentRuntimeOwnerIdentity {
        session_id: "owner".into(),
        agent_id: None,
    };
    let agent = json!({"id":"child","parentId":"main","displayName":"child","agentType":"background","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true},"detached":true,"suppressTerminalNotification":true});
    assert_eq!(
        call(
            &mut child,
            "createMultiAgentChildWithRuntimeOwnership",
            json!([db,{"sessionPath":"/fixture/s","agentId":"child","processIdentity":identity,"owner":owner,"nowIso":now,"agent":agent}])
        )["ok"],
        json!(true)
    );
    drop(child);
    fixture_process::wait_for_owned_fixture_exit(&identity);
    let supervisor_identity: ProcessIdentity =
        js_json::from_value(&js_json::parse_lossless(get_runtime_process_instance_id().unwrap()).unwrap()).unwrap();
    register_runtime_mailbox_listener(
        &db,
        &RuntimeMailboxAddress {
            session_id: "owner".into(),
            agent_id: None,
        },
        std::process::id(),
        Some(&JsString::from("/fixture/s")),
        RuntimeMailboxRegistrationOptions::default(),
    )
    .unwrap();
    let result = recover_dead_multi_agent_runtime(
        &db,
        &RecoverDeadMultiAgentRuntimeInput {
            expected_owner: MultiAgentRuntimeOwnershipIdentity {
                session_path: "/fixture/s".into(),
                agent_id: "child".into(),
                process_identity: identity,
                owner,
            },
            now_iso: now,
            supervisor: SupervisorRuntimeOwnership {
                agent_id: None,
                process_identity: supervisor_identity,
                session_id: "owner".into(),
            },
        },
    )
    .unwrap();
    assert!(
        matches!(result, RecoverDeadMultiAgentRuntimeResult::Ok { .. }),
        "{result:?}"
    );
    let mut peer = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert!(
        call(
            &mut peer,
            "listRuntimeMailboxMessagesForSession",
            json!([db, "/fixture/s"])
        )
        .as_array()
        .unwrap()
        .is_empty()
    );
}
