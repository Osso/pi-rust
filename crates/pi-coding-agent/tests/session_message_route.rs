use crate::shared_oracle as oracle;
use pi_coding_agent::bundled_extensions::agents_core::mailbox_runtime::SendAgentMessageParams;
use pi_coding_agent::bundled_extensions::agents_core::session_message_route::{
    is_remote_session_message, session_message_route_error,
};
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};

#[test]
fn local_remote_and_child_only_listener_routes_match_frozen_on_same_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("control.sqlite").to_str().unwrap().to_owned();
    let metadata:WritableSessionMetadata=js_json::from_value(&json!({
        "sessionPath":"/fixture/target.jsonl","id":"target","cwd":"/fixture",
        "createdAt":"2026-10-25T00:00:00.000Z","modifiedAt":"2026-10-25T00:00:00.000Z","messageCount":0,"firstMessage":""
    })).unwrap();
    write_session_metadata(&db, &metadata).unwrap();
    let hostname = gethostname::gethostname().to_str().unwrap().to_owned();
    let mut ts = oracle::TsOracle::spawn(
        "agents-core-repin-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    let check = |ts: &mut oracle::TsOracle, db: Option<&str>, params: JsValue| {
        let source = ts.request(&json!({"id":6,"op":"sessionRoute","db":db,"params":params}));
        assert_eq!(source["ok"], json!(true), "{source:?}");
        let native: SendAgentMessageParams = js_json::from_value(&params).unwrap();
        let db = db.map(JsString::from);
        let error = session_message_route_error(db.as_ref(), &native).unwrap();
        assert_eq!(
            source["output"],
            json!({"error":error,"remote":is_remote_session_message(&native)}),
            "{params:?}"
        );
    };
    let target = json!({"message":"literal","toAgentId":"main","toSessionId":"target"});
    check(&mut ts, Some(""), target.clone());
    check(&mut ts, None, target.clone());
    check(&mut ts, Some(&db), target.clone());
    check(
        &mut ts,
        Some(&db),
        json!({"message":"literal","toAgentId":"main","toSessionId":"unknown"}),
    );
    check(
        &mut ts,
        Some(&db),
        json!({"message":"literal","toAgentId":"main","toHost":" \t"}),
    );
    check(
        &mut ts,
        Some(&db),
        json!({"message":"literal","toAgentId":"main","toHost":"remote"}),
    );
    check(
        &mut ts,
        Some(&db),
        json!({"message":"literal","toAgentId":"main","toHost":"remote","toSessionId":"unknown"}),
    );
    let identity = pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).unwrap();
    for id in ["child-a", "child-b"] {
        register_runtime_mailbox_listener(
            &db,
            &RuntimeMailboxAddress {
                session_id: "target".into(),
                agent_id: Some(id.into()),
            },
            identity.pid,
            Some(&metadata.session_path),
            RuntimeMailboxRegistrationOptions {
                runtime_instance_id: Some(serialize_process_identity(&identity)),
                ..Default::default()
            },
        )
        .unwrap();
    }
    for target_id in ["main", "supervisor", "child-a"] {
        check(
            &mut ts,
            Some(&db),
            json!({"message":"literal","toAgentId":target_id,"toSessionId":"target"}),
        );
    }
    check(
        &mut ts,
        Some(&db),
        json!({"message":"literal","toAgentId":"main","toSessionId":"target","toHost":hostname}),
    );
    register_runtime_mailbox_listener(
        &db,
        &RuntimeMailboxAddress {
            session_id: "target".into(),
            agent_id: None,
        },
        identity.pid,
        Some(&metadata.session_path),
        RuntimeMailboxRegistrationOptions {
            runtime_instance_id: Some(serialize_process_identity(&identity)),
            ..Default::default()
        },
    )
    .unwrap();
    check(&mut ts, Some(&db), target);
}
