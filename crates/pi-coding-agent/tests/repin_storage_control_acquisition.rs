use super::repin_storage_history as oracle;
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::{
    AcquireAttachedRuntimeOwnershipInput, AcquireAttachedRuntimeOwnershipResult, MultiAgentRuntimeOwnerIdentity,
    RuntimeMailboxAddress, SupervisorRuntimeOwnership, acquire_attached_runtime_ownership, bootstrap_multi_agent_agent,
    get_runtime_process_instance_id, read_multi_agent_agent, read_multi_agent_runtime_ownership,
    register_runtime_mailbox_listener,
};
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};

#[test]
fn attached_runtime_acquisition_rejects_terminal_agents_without_changing_them() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let target = read_process_identity(std::process::id()).unwrap();
    let native_supervisor: ProcessIdentity = serde_json::from_str(get_runtime_process_instance_id().unwrap()).unwrap();
    for (case, lifecycle) in [
        "completed",
        "failed",
        "aborted",
        "waiting_for_input",
        "running",
        "steering_pending",
        "cancelling",
    ]
    .into_iter()
    .enumerate()
    {
        let source_db = root.path().join(format!("source{case}.sqlite"));
        let native_db = JsString::from(root.path().join(format!("native{case}.sqlite")).to_str().unwrap());
        let response=source.request(&json!({"id":1,"steps":[{"op":"registerSelf","args":[source_db,{"sessionId":"supervisor","agentId":null},"/main.jsonl"]}]}));
        let source_supervisor: Value = serde_json::from_str(response["output"][0]["value"].as_str().unwrap()).unwrap();
        let now = pi_coding_agent::utils::iso_time::now_iso_string();
        let agent = json!({"id":"child","parentId":"main","displayName":"Child","agentType":"worker","origin":"attached","lifecycle":lifecycle,"revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}});
        let response=source.request(&json!({"id":2,"steps":[
            {"op":"call","args":["bootstrapMultiAgentAgent",[source_db,"/main.jsonl","child",agent]]},
            {"op":"call","args":["acquireAttachedRuntimeOwnership",[source_db,{"sessionPath":"/main.jsonl","agentId":"child","processIdentity":target,"owner":{"sessionId":"supervisor","agentId":null},"nowIso":now,"supervisor":{"sessionId":"supervisor","agentId":null,"processIdentity":source_supervisor}}]]},
            {"op":"call","args":["readMultiAgentAgent",[source_db,"/main.jsonl","child"]]},
            {"op":"call","args":["readMultiAgentRuntimeOwnership",[source_db,"/main.jsonl","child"]]}
        ]}));
        assert_eq!(response["ok"], true, "{response}");
        register_runtime_mailbox_listener(
            &native_db,
            &RuntimeMailboxAddress {
                session_id: "supervisor".into(),
                agent_id: None,
            },
            std::process::id(),
            Some(&"/main.jsonl".into()),
            Default::default(),
        )
        .unwrap();
        bootstrap_multi_agent_agent(&native_db, "/main.jsonl", &"child".into(), &JsValue::from(agent)).unwrap();
        let result = acquire_attached_runtime_ownership(
            &native_db,
            &AcquireAttachedRuntimeOwnershipInput {
                session_path: "/main.jsonl".into(),
                agent_id: "child".into(),
                process_identity: target.clone(),
                owner: MultiAgentRuntimeOwnerIdentity {
                    session_id: "supervisor".into(),
                    agent_id: None,
                },
                now_iso: now,
                supervisor: SupervisorRuntimeOwnership {
                    session_id: "supervisor".into(),
                    agent_id: None,
                    process_identity: native_supervisor.clone(),
                },
            },
        )
        .unwrap();
        let expected = &response["output"][1]["value"];
        match result {
            AcquireAttachedRuntimeOwnershipResult::Ok { .. } => {
                assert_eq!(expected["ok"], true, "{lifecycle}: {expected}")
            }
            AcquireAttachedRuntimeOwnershipResult::Err { error } => {
                assert_eq!(json!({"ok":false,"error":error.as_str()}), *expected, "{lifecycle}")
            }
        }
        assert_eq!(
            JsValue::from(json!(
                read_multi_agent_agent(&native_db, "/main.jsonl", "child").unwrap()
            )),
            JsValue::from(response["output"][2]["value"].clone()),
            "{lifecycle}"
        );
        assert_eq!(
            JsValue::from(json!(
                read_multi_agent_runtime_ownership(&native_db, "/main.jsonl", &"child".into()).unwrap()
            )),
            JsValue::from(response["output"][3]["value"].clone()),
            "{lifecycle}"
        );
    }
}
