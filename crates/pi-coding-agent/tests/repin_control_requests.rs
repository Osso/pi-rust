#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use serde_json::{Value, json};
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], true, "{r}");
    r["output"][0]["value"].clone()
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
