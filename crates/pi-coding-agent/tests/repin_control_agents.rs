use crate::shared_oracle as oracle;
use pi_coding_agent::core::runtime_process::read_process_identity;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::{js_json, js_json as json};
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], json!(true), "{r:?}");
    r["output"][0]["value"].clone()
}
fn agent(now: &str) -> Value {
    json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}})
}
#[test]
fn upsert_keeps_original_birth_and_peer_claim_after_newer_snapshot() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let now = pi_js_compat::date::js_now_millis() as i64;
    let birth = pi_coding_agent::utils::iso_time::format_iso_millis(now - 1000);
    let message = json!({"id":"m","fromAgentId":"main","toAgentId":"child","kind":"message","status":"pending","createdAt":birth,"body":"hello"});
    call(
        &mut ts,
        "upsertMultiAgentMailboxMessage",
        json!([db, "/fixture/s", "m", message]),
    );
    let mut newer = message;
    newer["createdAt"] = json!(pi_coding_agent::utils::iso_time::format_iso_millis(now));
    upsert_multi_agent_mailbox_message(&db, "/fixture/s", "m", &newer).unwrap();
    let state = call(&mut ts, "readMultiAgentState", json!([db, "/fixture/s"]));
    assert_eq!(state["mailboxMessages"][0]["createdAt"], json!(birth));
    assert!(mark_multi_agent_mailbox_message_delivered(&db, "/fixture/s", &JsString::from("m")).unwrap());
    assert!(
        call(
            &mut ts,
            "getMultiAgentMailboxMessageStatus",
            json!([db, "/fixture/s", "m"])
        )
        .is_null()
    );
}
#[test]
fn rust_terminal_commit_peer_delete_and_identical_replay_do_not_require_outbox() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let identity = read_process_identity(std::process::id()).unwrap();
    let owner = MultiAgentRuntimeOwnerIdentity {
        session_id: "owner".into(),
        agent_id: None,
    };
    let created = call(
        &mut ts,
        "createMultiAgentChildWithRuntimeOwnership",
        json!([db,{"sessionPath":"/fixture/s","agentId":"child","processIdentity":identity,"owner":owner,"nowIso":now,"agent":agent(&now)}]),
    );
    assert_eq!(created["ok"], json!(true));
    let input = CommitMultiAgentTerminalMutationInput {
        session_path: "/fixture/s".into(),
        agent_id: "child".into(),
        process_identity: identity,
        owner,
        terminal_lifecycle: "completed".into(),
        event_kind: "completed".into(),
        agent_details: AgentDetails::default(),
        updated_at: now.clone(),
    };
    let result = commit_multi_agent_terminal_mutation(&db, &input).unwrap();
    assert!(matches!(result, CommitMultiAgentTerminalMutationResult::Ok { .. }));
    let record = call(&mut ts, "claimMultiAgentTerminalOutbox", json!([db, "claim", now]));
    assert!(record.is_object(), "{record:?}");
    assert_eq!(
        call(&mut ts, "deliverMultiAgentTerminalOutbox", json!([db, record, now])),
        json!(true)
    );
    assert_eq!(commit_multi_agent_terminal_mutation(&db, &input).unwrap(), result);
}
#[test]
fn rust_outbox_delivery_deletes_row_instead_of_leaving_delivered_projection() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let identity = read_process_identity(std::process::id()).unwrap();
    let owner = MultiAgentRuntimeOwnerIdentity {
        session_id: "owner".into(),
        agent_id: None,
    };
    let created = call(
        &mut ts,
        "createMultiAgentChildWithRuntimeOwnership",
        json!([db,{"sessionPath":"/fixture/s","agentId":"child","processIdentity":identity,"owner":owner,"nowIso":now,"agent":agent(&now)}]),
    );
    assert_eq!(created["ok"], json!(true));
    commit_multi_agent_terminal_mutation(
        &db,
        &CommitMultiAgentTerminalMutationInput {
            session_path: "/fixture/s".into(),
            agent_id: "child".into(),
            process_identity: identity,
            owner,
            terminal_lifecycle: "completed".into(),
            event_kind: "completed".into(),
            agent_details: AgentDetails::default(),
            updated_at: now.clone(),
        },
    )
    .unwrap();
    let record = claim_multi_agent_terminal_outbox(
        &db,
        &JsString::from("rust"),
        &now,
        &ClaimMultiAgentTerminalOutboxOptions::default(),
    )
    .unwrap()
    .unwrap();
    assert!(deliver_multi_agent_terminal_outbox(&db, &record).unwrap());
    assert!(!deliver_multi_agent_terminal_outbox(&db, &record).unwrap());
    let response=ts.request(&json!({"id":1,"steps":[{"op":"fixtureSql","args":[db,"SELECT count(*) AS total FROM multi_agent_terminal_outbox",[]]}]}));
    assert_eq!(response["output"][0]["value"][0]["total"], json!(0));
}

#[test]
fn owned_model_and_tool_detail_updates_are_visible_to_same_ts_reader() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let identity = read_process_identity(std::process::id()).unwrap();
    let owner = MultiAgentRuntimeOwnerIdentity {
        session_id: "owner".into(),
        agent_id: None,
    };
    assert_eq!(
        call(
            &mut ts,
            "createMultiAgentChildWithRuntimeOwnership",
            json!([db,{"sessionPath":"/fixture/s","agentId":"child","processIdentity":identity,"owner":owner,"nowIso":now,"agent":agent(&now)}])
        )["ok"],
        json!(true)
    );
    let mut activity_owner = pi_coding_agent::core::multi_agent_store::AgentCurrentActivityOwner {
        owner_session_id: "wrong".into(),
        process_identity: identity,
    };
    let model = json!({"providerId":"fixture","modelId":"model","thinkingLevel":"high"});
    assert!(
        update_multi_agent_agent_model(
            &db,
            "/fixture/s",
            &JsString::from("child"),
            &model,
            &now,
            &activity_owner
        )
        .unwrap()
        .is_none()
    );
    activity_owner.owner_session_id = "owner".into();
    assert!(
        update_multi_agent_agent_model(
            &db,
            "/fixture/s",
            &JsString::from("child"),
            &model,
            &now,
            &activity_owner
        )
        .unwrap()
        .is_some()
    );
    let tool = json!({"phase":"tool","startedAt":now,"toolCallId":"t","toolName":"read","detail":"/fixture/a.rs"});
    update_multi_agent_agent_current_activity(
        &db,
        "/fixture/s",
        &JsString::from("child"),
        Some(&tool),
        &now,
        &activity_owner,
        Some(&model),
    )
    .unwrap()
    .unwrap();
    let peer = call(&mut ts, "readMultiAgentAgent", json!([db, "/fixture/s", "child"]));
    assert_eq!(peer["currentActivity"], tool);
    assert_eq!(peer["model"], model);
    assert_eq!(
        js_json::to_value(&read_multi_agent_agent(&db, "/fixture/s", "child").unwrap().unwrap()).unwrap()["currentActivity"],
        tool
    );
}
