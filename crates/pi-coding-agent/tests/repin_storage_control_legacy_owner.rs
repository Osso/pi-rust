use super::repin_storage_history as oracle;
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::{
    AcquireAttachedRuntimeOwnershipInput, AcquireAttachedRuntimeOwnershipResult, MultiAgentRuntimeOwnerIdentity,
    SupervisorRuntimeOwnership, acquire_attached_runtime_ownership, get_runtime_process_instance_id,
    read_multi_agent_agent,
};
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::json;

#[test]
fn a_replacement_without_nonce_cannot_supersede_an_alive_nonce_owner() {
    let root = tempfile::tempdir().unwrap();
    let db = JsString::from(root.path().join("control.sqlite").to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let supervisor: ProcessIdentity = serde_json::from_str(get_runtime_process_instance_id().unwrap()).unwrap();
    let replacement = read_process_identity(std::process::id()).unwrap();
    let mut current = replacement.clone();
    current.incarnation = Some("old-incarnation".into());
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let agent = json!({"id":"child","parentId":"main","displayName":"Child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}});
    let input = json!({"sessionPath":"/main.jsonl","agentId":"child","processIdentity":replacement,"owner":{"sessionId":"supervisor","agentId":null},"nowIso":now,"supervisor":{"sessionId":"supervisor","agentId":null,"processIdentity":supervisor}});
    let response=source.request(&json!({"id":1,"steps":[
        {"op":"call","args":["registerRuntimeMailboxListener",[db,{"sessionId":"supervisor","agentId":null},std::process::id(),"/main.jsonl",{"runtimeInstanceId":get_runtime_process_instance_id().unwrap()}]]},
        {"op":"call","args":["createMultiAgentChildWithRuntimeOwnership",[db,{"sessionPath":"/main.jsonl","agentId":"child","processIdentity":current,"owner":{"sessionId":"supervisor","agentId":null},"nowIso":now,"agent":agent}]]},
        {"op":"call","args":["acquireAttachedRuntimeOwnership",[db,input]]}
    ]}));
    assert_eq!(
        response["output"][2]["value"],
        json!({"ok":false,"error":"ownership_held"}),
        "{response}"
    );
    let result = acquire_attached_runtime_ownership(
        &db,
        &AcquireAttachedRuntimeOwnershipInput {
            session_path: "/main.jsonl".into(),
            agent_id: "child".into(),
            process_identity: replacement,
            owner: MultiAgentRuntimeOwnerIdentity {
                session_id: "supervisor".into(),
                agent_id: None,
            },
            now_iso: now,
            supervisor: SupervisorRuntimeOwnership {
                session_id: "supervisor".into(),
                agent_id: None,
                process_identity: supervisor,
            },
        },
    )
    .unwrap();
    match result {
        AcquireAttachedRuntimeOwnershipResult::Err { error } => assert_eq!(error.as_str(), "ownership_held"),
        other => panic!("expected ownership_held, got {other:?}"),
    }
    assert_eq!(
        JsValue::from(json!(read_multi_agent_agent(&db, "/main.jsonl", "child").unwrap())),
        JsValue::from(agent)
    );
}
