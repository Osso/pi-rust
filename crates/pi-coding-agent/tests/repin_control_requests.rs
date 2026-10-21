#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use serde_json::{Value, json};
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], true, "{r}");
    r["output"][0]["value"].clone()
}
fn response_error(ts: &mut oracle::TsOracle, name: &str, args: Value) -> String {
    let response = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(response["output"][0]["ok"], false, "{response}");
    response["output"][0]["error"]["message"].as_str().unwrap().into()
}

#[test]
fn supervisor_priority_exclusive_claim_requeue_and_response_kind_match_peer() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("control.sqlite").display().to_string();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let future =
        pi_coding_agent::utils::iso_time::format_iso_millis(pi_js_compat::date::js_now_millis() as i64 + 3_600_000);
    let mut ids = Vec::new();
    for kind in ["supervisor_advisory", "goal_idle_review", "approval_review"] {
        ids.push(call(&mut source,"postSupervisorRequest",json!([path,{"senderSessionId":"sender","projectId":"project","kind":kind,"payload":{"concrete":"fixture"},"deadlineAt":future}])).as_i64().unwrap());
    }
    assert!(has_pending_supervisor_approval_request(&path).unwrap());
    let approval = claim_next_supervisor_request(&path, "native-first").unwrap().unwrap();
    assert_eq!(approval.id, ids[2]);
    assert!(
        call(
            &mut source,
            "claimNextSupervisorRequest",
            json!([path, "peer-exclusive"])
        )
        .is_null()
    );
    assert!(!has_pending_supervisor_approval_request(&path).unwrap());
    let error = requeue_supervisor_request(&path, approval.id, "foreign").unwrap_err();
    assert_eq!(
        error.message(),
        response_error(
            &mut source,
            "requeueSupervisorRequest",
            json!([path, approval.id, "foreign"])
        )
    );
    let invalid = SupervisorResponse::Advisory {
        answer: "wrong response domain".into(),
    };
    let error = complete_supervisor_request(&path, approval.id, "native-first", &invalid).unwrap_err();
    assert_eq!(
        error.message(),
        response_error(
            &mut source,
            "completeSupervisorRequest",
            json!([path, approval.id, "native-first", invalid])
        )
    );
    assert_eq!(
        call(&mut source, "readSupervisorRequest", json!([path, approval.id]))["status"],
        "claimed"
    );
    call(
        &mut source,
        "requeueSupervisorRequest",
        json!([path, approval.id, "native-first"]),
    );
    let peer = call(&mut source, "claimNextSupervisorRequest", json!([path, "peer-final"]));
    assert_eq!(peer["id"], approval.id);
    complete_supervisor_request(
        &path,
        approval.id,
        "peer-final",
        &SupervisorResponse::Approve {
            reason: "allowed".into(),
        },
    )
    .unwrap();
    assert_eq!(
        call(&mut source, "readSupervisorRequest", json!([path, approval.id]))["response"],
        json!({"kind":"approve","reason":"allowed"})
    );
    let goal = claim_next_supervisor_request(&path, "goal").unwrap().unwrap();
    assert_eq!(goal.id, ids[1]);
    call(
        &mut source,
        "completeSupervisorRequest",
        json!([path,goal.id,"goal",{"kind":"continue","reason":"continue concrete goal","instructions":"next bounded task"}]),
    );
    assert_eq!(
        claim_next_supervisor_request(&path, "advisory").unwrap().unwrap().id,
        ids[0]
    );
}

#[test]
fn supervisor_recovery_expires_deadlines_and_requeues_unexpired_peer_claims() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("control.sqlite").display().to_string();
    // Native initializer first; source public producers and claims then mutate the same database.
    assert!(read_supervisor_request(&path, -1).unwrap().is_none());
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let now = pi_js_compat::date::js_now_millis() as i64;
    let past = pi_coding_agent::utils::iso_time::format_iso_millis(now - 3_600_000);
    let future = pi_coding_agent::utils::iso_time::format_iso_millis(now + 3_600_000);
    let expired = call(&mut source,"postSupervisorRequest",json!([path,{"senderSessionId":"sender","projectId":"project","kind":"supervisor_advisory","payload":{},"deadlineAt":past}])).as_i64().unwrap();
    let future_id = call(&mut source,"postSupervisorRequest",json!([path,{"senderSessionId":"sender","projectId":"project","kind":"approval_review","payload":{},"deadlineAt":future}])).as_i64().unwrap();
    assert_eq!(
        call(
            &mut source,
            "claimNextSupervisorRequest",
            json!([path, "dead-peer-claim"])
        )["id"],
        future_id
    );
    recover_supervisor_requests(&path).unwrap();
    let expired_row = call(&mut source, "readSupervisorRequest", json!([path, expired]));
    assert_eq!(expired_row["status"], "completed");
    assert_eq!(
        expired_row["response"],
        json!({"kind":"error","reason":"Supervisor request deadline expired"})
    );
    let requeued = read_supervisor_request(&path, future_id).unwrap().unwrap();
    assert_eq!(requeued.status, SupervisorRequestStatus::Pending);
    assert_eq!(requeued.claim_token, None);
    assert_eq!(requeued.claimed_at, None);
    assert_eq!(
        call(&mut source, "readSupervisorRequest", json!([path, future_id]))["status"],
        "pending"
    );
    let old = SupervisorResponse::Approve {
        reason: "late owner".into(),
    };
    assert!(complete_supervisor_request(&path, future_id, "dead-peer-claim", &old).is_err());
    let claimed = claim_next_supervisor_request(&path, "replacement").unwrap().unwrap();
    assert_eq!(claimed.id, future_id);
    call(
        &mut source,
        "completeSupervisorRequest",
        json!([path,future_id,"replacement",{"kind":"error","reason":"explicit failure"}]),
    );
    recover_supervisor_requests(&path).unwrap();
    assert_eq!(
        call(&mut source, "readSupervisorRequest", json!([path, future_id]))["response"],
        json!({"kind":"error","reason":"explicit failure"})
    );
}

#[test]
fn cancelled_peer_request_stays_cancelled_and_rejects_completion() {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let deadline =
        pi_coding_agent::utils::iso_time::format_iso_millis(pi_js_compat::date::js_now_millis() as i64 + 3600_000);
    let id=call(&mut ts,"postSupervisorRequest",json!([db,{"senderSessionId":"sender","projectId":"p","kind":"supervisor_advisory","payload":{},"deadlineAt":deadline}])).as_i64().unwrap();
    let claim = claim_next_supervisor_request(&db, "claim").unwrap().unwrap();
    assert_eq!(claim.id, id);
    assert_eq!(
        call(
            &mut ts,
            "cancelSupervisorRequest",
            json!([db, id, "sender", "cancel reason"])
        ),
        true
    );
    let row = read_supervisor_request(&db, id).unwrap().unwrap();
    assert_eq!(serde_json::to_value(row.status).unwrap(), "cancelled");
    assert!(
        complete_supervisor_request(
            &db,
            id,
            "claim",
            &SupervisorResponse::Advisory { answer: "late".into() }
        )
        .is_err()
    );
    assert_eq!(
        call(&mut ts, "readSupervisorRequest", json!([db, id]))["response"],
        json!({"kind":"error","reason":"cancel reason"})
    );
    let input:PostSupervisorRequestInput=serde_json::from_value(json!({"senderSessionId":"rust","projectId":"p","kind":"supervisor_advisory","payload":{},"deadlineAt":deadline})).unwrap();
    let rust = post_supervisor_request(&db, &input).unwrap();
    assert!(!cancel_supervisor_request(&db, rust, "wrong", "no").unwrap());
    assert!(cancel_supervisor_request(&db, rust, "rust", "Rust cancelled").unwrap());
    assert!(!cancel_supervisor_request(&db, rust, "rust", "retry").unwrap());
    assert_eq!(
        call(&mut ts, "readSupervisorRequest", json!([db, rust]))["status"],
        "cancelled"
    );
}
