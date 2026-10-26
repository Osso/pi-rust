use crate::shared_oracle as oracle;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::{js_json, js_json as json};
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], json!(true), "{r:?}");
    r["output"][0]["value"].clone()
}
fn response_error(ts: &mut oracle::TsOracle, name: &str, args: Value) -> JsString {
    let response = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(response["output"][0]["ok"], json!(false), "{response:?}");
    response["output"][0]["error"]["message"].as_string().unwrap().clone()
}

fn claimed_goal_request(source: &mut oracle::TsOracle, path: &str) -> i64 {
    let id = call(
        source,
        "postSupervisorRequest",
        json!([path, {"senderSessionId":"sender","projectId":"project","kind":"goal_idle_review",
            "payload":{"question":"next"},"deadlineAt":"2099-01-01T00:00:00.000Z"}]),
    )
    .as_f64()
    .unwrap() as i64;
    assert_eq!(
        call(source, "claimNextSupervisorRequest", json!([path, "claim"]))["id"],
        json!(id)
    );
    id
}

fn assert_supervisor_response_wire(source: &mut oracle::TsOracle, path: &str, id: i64, expected: &str) {
    let peer = source.request(&json!({"id":1,"steps":[
        {"op":"callWire","args":["readSupervisorRequest",js_json::stringify_lossless(&json!([path,id]))]},
        {"op":"fixtureSql","args":[path,"SELECT response_json FROM supervisor_requests WHERE id = ?",[id]]}
    ]}));
    assert_eq!(peer["output"][0]["ok"], json!(true), "{peer:?}");
    assert_eq!(peer["output"][1]["ok"], json!(true), "{peer:?}");
    assert_eq!(peer["output"][1]["value"][0]["response_json"], json!(expected));
    let peer_wire = peer["output"][0]["value"].as_string().unwrap().to_utf8().unwrap();
    let native = read_supervisor_request(path, id).unwrap().unwrap();
    let native_wire = js_json::stringify_lossless(&js_json::to_value(&native).unwrap());
    assert_eq!(native_wire, peer_wire);
    let native_response = js_json::to_value(&native.response.unwrap()).unwrap();
    assert_eq!(js_json::stringify_lossless(&native_response), expected);
}

#[test]
fn supervisor_ts_completion_preserves_response_wire_on_native_read() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("control.sqlite");
    let path = path.to_str().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for expected in [
        r#"{"kind":"continue","reason":"more work","instructions":"run the next task"}"#,
        r#"{"kind":"continue","extra":{"z":"\ud800","a":2},"reason":"more work","instructions":"run the next task"}"#,
    ] {
        let id = claimed_goal_request(&mut source, path);
        let response = js_json::parse_lossless(expected).unwrap();
        call(
            &mut source,
            "completeSupervisorRequest",
            json!([path, id, "claim", response]),
        );
        assert_supervisor_response_wire(&mut source, path, id, expected);
    }
}

#[test]
fn supervisor_native_completion_preserves_ts_response_wire_in_sqlite_and_peer_read() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("control.sqlite");
    let path = path.to_str().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for expected in [
        r#"{"kind":"continue","reason":"more work","instructions":"run the next task"}"#,
        r#"{"kind":"continue","extra":{"z":"\ud800","a":2},"reason":"more work","instructions":"run the next task"}"#,
    ] {
        let id = claimed_goal_request(&mut source, path);
        let response = js_json::parse_lossless(expected).unwrap();
        complete_supervisor_request(path, id, &JsString::from("claim"), &response).unwrap();
        assert_supervisor_response_wire(&mut source, path, id, expected);
    }
}

#[test]
fn supervisor_priority_exclusive_claim_requeue_and_response_kind_match_peer() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let future =
        pi_coding_agent::utils::iso_time::format_iso_millis(pi_js_compat::date::js_now_millis() as i64 + 3_600_000);
    let mut ids = Vec::new();
    for kind in ["supervisor_advisory", "goal_idle_review", "approval_review"] {
        ids.push(call(&mut source,"postSupervisorRequest",json!([path,{"senderSessionId":"sender","projectId":"project","kind":kind,"payload":{"concrete":"fixture"},"deadlineAt":future}])).as_f64().unwrap() as i64);
    }
    assert!(has_pending_supervisor_approval_request(&path).unwrap());
    let approval = claim_next_supervisor_request(&path, &JsString::from("native-first"))
        .unwrap()
        .unwrap();
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
    let error = requeue_supervisor_request(&path, approval.id, &JsString::from("foreign")).unwrap_err();
    assert_eq!(
        *error.message(),
        response_error(
            &mut source,
            "requeueSupervisorRequest",
            json!([path, approval.id, "foreign"])
        )
    );
    let invalid = json!({"answer":"wrong response domain","kind":"advisory"});
    let error = complete_supervisor_request(&path, approval.id, &JsString::from("native-first"), &invalid).unwrap_err();
    assert_eq!(
        *error.message(),
        response_error(
            &mut source,
            "completeSupervisorRequest",
            json!([path, approval.id, "native-first", invalid])
        )
    );
    assert_eq!(
        call(&mut source, "readSupervisorRequest", json!([path, approval.id]))["status"],
        json!("claimed")
    );
    call(
        &mut source,
        "requeueSupervisorRequest",
        json!([path, approval.id, "native-first"]),
    );
    let peer = call(&mut source, "claimNextSupervisorRequest", json!([path, "peer-final"]));
    assert_eq!(peer["id"], json!(approval.id));
    complete_supervisor_request(
        &path,
        approval.id,
        &JsString::from("peer-final"),
        &json!({"kind":"approve","reason":"allowed"}),
    )
    .unwrap();
    assert_eq!(
        call(&mut source, "readSupervisorRequest", json!([path, approval.id]))["response"],
        json!({"kind":"approve","reason":"allowed"})
    );
    let goal = claim_next_supervisor_request(&path, &JsString::from("goal"))
        .unwrap()
        .unwrap();
    assert_eq!(goal.id, ids[1]);
    call(
        &mut source,
        "completeSupervisorRequest",
        json!([path,goal.id,"goal",{"kind":"continue","reason":"continue concrete goal","instructions":"next bounded task"}]),
    );
    assert_eq!(
        claim_next_supervisor_request(&path, &JsString::from("advisory"))
            .unwrap()
            .unwrap()
            .id,
        ids[0]
    );
}

#[test]
fn supervisor_recovery_expires_deadlines_and_requeues_unexpired_peer_claims() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    // Native initializer first; source public producers and claims then mutate the same database.
    assert!(read_supervisor_request(&path, -1).unwrap().is_none());
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let now = pi_js_compat::date::js_now_millis() as i64;
    let past = pi_coding_agent::utils::iso_time::format_iso_millis(now - 3_600_000);
    let future = pi_coding_agent::utils::iso_time::format_iso_millis(now + 3_600_000);
    let expired = call(&mut source,"postSupervisorRequest",json!([path,{"senderSessionId":"sender","projectId":"project","kind":"supervisor_advisory","payload":{},"deadlineAt":past}])).as_f64().unwrap() as i64;
    let future_id = call(&mut source,"postSupervisorRequest",json!([path,{"senderSessionId":"sender","projectId":"project","kind":"approval_review","payload":{},"deadlineAt":future}])).as_f64().unwrap() as i64;
    assert_eq!(
        call(
            &mut source,
            "claimNextSupervisorRequest",
            json!([path, "dead-peer-claim"])
        )["id"],
        json!(future_id)
    );
    recover_supervisor_requests(&path).unwrap();
    let expired_row = call(&mut source, "readSupervisorRequest", json!([path, expired]));
    assert_eq!(expired_row["status"], json!("completed"));
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
        json!("pending")
    );
    let old = json!({"kind":"approve","reason":"late owner"});
    assert!(complete_supervisor_request(&path, future_id, &JsString::from("dead-peer-claim"), &old).is_err());
    let claimed = claim_next_supervisor_request(&path, &JsString::from("replacement"))
        .unwrap()
        .unwrap();
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
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let deadline =
        pi_coding_agent::utils::iso_time::format_iso_millis(pi_js_compat::date::js_now_millis() as i64 + 3_600_000);
    let id=call(&mut ts,"postSupervisorRequest",json!([db,{"senderSessionId":"sender","projectId":"p","kind":"supervisor_advisory","payload":{},"deadlineAt":deadline}])).as_f64().unwrap() as i64;
    let claim = claim_next_supervisor_request(&db, &JsString::from("claim"))
        .unwrap()
        .unwrap();
    assert_eq!(claim.id, id);
    assert_eq!(
        call(
            &mut ts,
            "cancelSupervisorRequest",
            json!([db, id, "sender", "cancel reason"])
        ),
        json!(true)
    );
    let row = read_supervisor_request(&db, id).unwrap().unwrap();
    assert_eq!(js_json::to_value(&row.status).unwrap(), json!("cancelled"));
    assert!(
        complete_supervisor_request(
            &db,
            id,
            &JsString::from("claim"),
            &json!({"answer":"late","kind":"advisory"})
        )
        .is_err()
    );
    assert_eq!(
        call(&mut ts, "readSupervisorRequest", json!([db, id]))["response"],
        json!({"kind":"error","reason":"cancel reason"})
    );
    let input:PostSupervisorRequestInput=js_json::from_value(&json!({"senderSessionId":"rust","projectId":"p","kind":"supervisor_advisory","payload":{},"deadlineAt":deadline})).unwrap();
    let rust = post_supervisor_request(&db, &input).unwrap();
    assert!(!cancel_supervisor_request(&db, rust, &JsString::from("wrong"), "no").unwrap());
    assert!(cancel_supervisor_request(&db, rust, &JsString::from("rust"), "Rust cancelled").unwrap());
    assert!(!cancel_supervisor_request(&db, rust, &JsString::from("rust"), "retry").unwrap());
    assert_eq!(
        call(&mut ts, "readSupervisorRequest", json!([db, rust]))["status"],
        json!("cancelled")
    );
}
