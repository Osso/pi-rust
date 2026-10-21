//! Fresh Rust public producer → same frozen TS initializes → Rust mutates → same TS reads.
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use serde_json::{Value, json};
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], true, "{r}");
    r["output"][0]["value"].clone()
}
fn temp() -> (tempfile::TempDir, String) {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    (t, db)
}
#[test]
fn fresh_rust_mailbox_survives_ts_init_claim_fail_and_explicit_delete() {
    let (_t, db) = temp();
    let input:EnqueueStoredRuntimeMailboxMessageInput=serde_json::from_value(json!({"recipient":{"sessionId":"to","agentId":null},"sender":{"sessionId":"from","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/s","messageId":"m"},"message":{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"Rust first"}})).unwrap();
    let id = enqueue_stored_runtime_mailbox_message(&db, &input).unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["body"],
        "Rust first"
    );
    assert_eq!(enqueue_stored_runtime_mailbox_message(&db, &input).unwrap(), id);
    register_runtime_mailbox_listener(&db, &input.recipient, std::process::id(), None, Default::default()).unwrap();
    assert_eq!(
        claim_runtime_mailbox_messages(&db, &input.recipient, 1).unwrap().len(),
        1
    );
    fail_runtime_mailbox_message(&db, id, "fixture failure").unwrap();
    fail_runtime_mailbox_message(&db, id, "later error").unwrap();
    let peer = call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]));
    assert_eq!(peer["status"], "failed");
    assert_eq!(peer["error"], "fixture failure");
    mark_runtime_mailbox_message_delivered(&db, id).unwrap();
    assert!(call(&mut ts, "readRuntimeMailboxMessage", json!([db, id])).is_null());
}
#[test]
fn fresh_rust_owned_agent_model_terminal_and_peer_ack_replay() {
    let (_t, db) = temp();
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let identity = pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).unwrap();
    let owner = MultiAgentRuntimeOwnerIdentity {
        session_id: "owner".into(),
        agent_id: None,
    };
    let agent=serde_json::from_value(json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}})).unwrap();
    assert!(matches!(
        create_multi_agent_child_with_runtime_ownership(
            &db,
            &CreateMultiAgentChildWithRuntimeOwnershipInput {
                session_path: "/fixture/s".into(),
                agent_id: "child".into(),
                process_identity: identity.clone(),
                owner: owner.clone(),
                now_iso: now.clone(),
                agent
            }
        )
        .unwrap(),
        CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { .. }
    ));
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut ts, "readMultiAgentAgent", json!([db, "/fixture/s", "child"]))["revision"],
        1
    );
    let model = json!({"providerId":"fixture","modelId":"effective","thinkingLevel":"high"});
    let activity_owner = pi_coding_agent::core::multi_agent_store::AgentCurrentActivityOwner {
        owner_session_id: "owner".into(),
        process_identity: identity.clone(),
    };
    update_multi_agent_agent_model(&db, "/fixture/s", "child", &model, &now, &activity_owner)
        .unwrap()
        .unwrap();
    assert_eq!(
        call(&mut ts, "readMultiAgentAgent", json!([db, "/fixture/s", "child"]))["model"],
        model
    );
    let terminal = CommitMultiAgentTerminalMutationInput {
        session_path: "/fixture/s".into(),
        agent_id: "child".into(),
        process_identity: identity,
        owner,
        terminal_lifecycle: "completed".into(),
        event_kind: "completed".into(),
        agent_details: Default::default(),
        updated_at: now.clone(),
    };
    let first = commit_multi_agent_terminal_mutation(&db, &terminal).unwrap();
    let record = call(&mut ts, "claimMultiAgentTerminalOutbox", json!([db, "peer", now]));
    assert_eq!(
        call(&mut ts, "deliverMultiAgentTerminalOutbox", json!([db, record])),
        true
    );
    assert_eq!(commit_multi_agent_terminal_mutation(&db, &terminal).unwrap(), first);
}
#[test]
fn fresh_rust_channel_and_relay_cursor_names_coexist_after_ts_init() {
    let (_t, db) = temp();
    let sender = RuntimeMailboxAddress {
        session_id: "same".into(),
        agent_id: None,
    };
    let first = post_shared_channel_message(
        &db,
        &PostSharedChannelMessageInput {
            sender: sender.clone(),
            body: "Rust first".into(),
        },
    )
    .unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut ts, "listSharedChannelMessagesAfter", json!([db, 0, 20]))[0]["body"],
        "Rust first"
    );
    advance_shared_channel_cursor(&db, &sender, first).unwrap();
    assert_eq!(call(&mut ts, "readSharedChannelCursor", json!([db, sender])), first);
    let relay = RuntimeMailboxAddress {
        session_id: "relay-in:fixture-peer".into(),
        agent_id: None,
    };
    call(&mut ts, "advanceSharedChannelCursor", json!([db, relay, 17]));
    let second = call(
        &mut ts,
        "postSharedChannelMessage",
        json!([db,{"sender":{"sessionId":"peer","agentId":null},"body":"TS next"}]),
    )
    .as_i64()
    .unwrap();
    assert_eq!(
        list_shared_channel_messages_after(&db, first, 20, i64::MAX).unwrap()[0].id,
        second
    );
    advance_shared_channel_cursor(&db, &sender, second).unwrap();
    advance_shared_channel_cursor(&db, &sender, 0).unwrap();
    assert_eq!(call(&mut ts, "readSharedChannelCursor", json!([db, sender])), second);
    assert_eq!(call(&mut ts, "readSharedChannelCursor", json!([db, relay])), 17);
}
#[test]
fn fresh_rust_supervisor_request_remains_cancelled_after_peer_takeover() {
    let (_t, db) = temp();
    let deadline =
        pi_coding_agent::utils::iso_time::format_iso_millis(pi_js_compat::date::js_now_millis() as i64 + 3_600_000);
    let input:PostSupervisorRequestInput=serde_json::from_value(json!({"senderSessionId":"sender","projectId":"p","kind":"supervisor_advisory","payload":{"question":"fixture"},"deadlineAt":deadline})).unwrap();
    let id = post_supervisor_request(&db, &input).unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut ts, "readSupervisorRequest", json!([db, id]))["status"],
        "pending"
    );
    claim_next_supervisor_request(&db, "rust-claim").unwrap().unwrap();
    assert_eq!(
        call(
            &mut ts,
            "cancelSupervisorRequest",
            json!([db, id, "sender", "peer cancel"])
        ),
        true
    );
    assert!(
        complete_supervisor_request(
            &db,
            id,
            "rust-claim",
            &SupervisorResponse::Advisory { answer: "late".into() }
        )
        .is_err()
    );
    assert_eq!(
        call(&mut ts, "readSupervisorRequest", json!([db, id]))["status"],
        "cancelled"
    );
}
#[test]
fn unchanged_incoming_history_health_and_counters_cross_public_producers() {
    let (_t, db) = temp();
    let id = enqueue_incoming_message(&db, "native incoming").unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert_eq!(call(&mut ts, "claimLatestIncomingMessage", json!([db]))["id"], id);
    complete_incoming_message(&db, id).unwrap();
    assert_eq!(call(&mut ts, "readIncomingMessageStatus", json!([db, id])), "completed");
    record_prompt_history_entry(&db, "same").unwrap();
    call(&mut ts, "recordPromptHistoryEntry", json!([db, "same"]));
    assert_eq!(read_prompt_history(&db, 10).unwrap(), vec!["same"]);
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let health=serde_json::from_value(json!({"sessionId":"health","agentGeneration":2,"pid":std::process::id(),"lastActiveAt":now,"lastCheckedAt":now,"checkStatus":"ok","checkedGeneration":2,"checkLatencyMs":3,"updatedAt":now})).unwrap();
    write_session_health(&db, &health).unwrap();
    assert_eq!(
        call(&mut ts, "readSessionHealth", json!([db, "health"])),
        serde_json::to_value(&health).unwrap()
    );
    assert_eq!(
        allocate_multi_agent_counter(&db, "/fixture/s", MultiAgentCounterName::Agent).unwrap(),
        1
    );
    assert_eq!(
        call(
            &mut ts,
            "allocateMultiAgentCounter",
            json!([db, "/fixture/s", "message"])
        ),
        1
    );
    assert_eq!(
        read_multi_agent_state(&db, "/fixture/s")
            .unwrap()
            .unwrap()
            .counters
            .next_message_number,
        2
    );
}
#[test]
fn archive_failure_after_relocation_rolls_back_metadata_and_shared_rows() {
    let (_t, db) = temp();
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let metadata:WritableSessionMetadata=serde_json::from_value(json!({"sessionPath":"/fixture/s","id":"s","cwd":"/fixture","name":"original","createdAt":now,"modifiedAt":now,"messageCount":1,"firstMessage":"body"})).unwrap();
    write_session_metadata(&db, &metadata).unwrap();
    allocate_multi_agent_counter(&db, "/fixture/s", MultiAgentCounterName::Agent).unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut ts, "readSessionMetadata", json!([db, "/fixture/s"]))["name"],
        "original"
    );
    let response=ts.request(&json!({"id":1,"steps":[{"op":"fixtureExec","args":[db,"CREATE TRIGGER fixture_archive_fault BEFORE UPDATE OF archived_at ON session_metadata BEGIN SELECT RAISE(ABORT,'fixture archive fault'); END;"]}]}));
    assert_eq!(response["output"][0]["ok"], true, "{response}");
    assert!(relocate_and_archive_session(&db, "/fixture/s", "/fixture/s.zst").is_err());
    assert_eq!(
        call(&mut ts, "readSessionMetadata", json!([db, "/fixture/s"]))["name"],
        "original"
    );
    assert!(call(&mut ts, "readSessionMetadata", json!([db, "/fixture/s.zst"])).is_null());
    assert!(call(&mut ts, "readMultiAgentState", json!([db, "/fixture/s.zst"])).is_null());
    assert_eq!(
        call(&mut ts, "readMultiAgentState", json!([db, "/fixture/s"]))["counters"]["nextAgentNumber"],
        2
    );
}

#[test]
fn last_message_roundtrip_keeps_assistant_singleton_content() {
    let (_t, db) = temp();
    write_last_message(&db, "assistant", "  native\n\n").unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut ts, "readLastMessage", json!([db])),
        serde_json::to_value(read_last_message(&db).unwrap().unwrap()).unwrap()
    );
    call(
        &mut ts,
        "writeLastMessage",
        json!([db,{"role":"assistant","content":"peer content"}]),
    );
    assert_eq!(read_last_message(&db).unwrap().unwrap().content, "peer content");
}

#[test]
fn sandbox_profile_identity_relocation_and_metadata_delete_match_peer() {
    use pi_coding_agent::core::permissions::presets::SandboxProfileName;
    let (_t, db) = temp();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    call(
        &mut ts,
        "writeSessionMetadata",
        json!([db,{"sessionPath":"/fixture/s","id":"s","cwd":"/fixture","createdAt":now,"modifiedAt":now,"messageCount":1,"firstMessage":"body"}]),
    );
    write_session_sandbox_profile(&db, "/fixture/s", "s", SandboxProfileName::ReadOnly).unwrap();
    assert_eq!(
        call(&mut ts, "readSessionSandboxProfile", json!([db, "/fixture/s", "s"])),
        "read-only"
    );
    assert!(clear_session_sandbox_profile(&db, "/fixture/s", "wrong").is_err());
    assert_eq!(
        call(&mut ts, "readSessionSandboxProfile", json!([db, "/fixture/s", "s"])),
        "read-only"
    );
    relocate_and_archive_session(&db, "/fixture/s", "/fixture/s.zst").unwrap();
    assert_eq!(
        call(&mut ts, "readSessionSandboxProfile", json!([db, "/fixture/s.zst", "s"])),
        "read-only"
    );
    remove_session_metadata(&db, "/fixture/s.zst").unwrap();
    assert!(call(&mut ts, "readSessionSandboxProfile", json!([db, "/fixture/s.zst", "s"])).is_null());
}
