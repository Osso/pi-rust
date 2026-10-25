use super::repin_storage_history as oracle;
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::{
    MultiAgentRuntimeOwnerIdentity, MultiAgentRuntimeOwnershipIdentity, RecoverDeadMultiAgentRuntimeInput,
    RecoverDeadMultiAgentRuntimeResult, SupervisorRuntimeOwnership, get_runtime_process_instance_id,
    read_multi_agent_agent, recover_dead_multi_agent_runtime, with_control_db,
};
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::json;

fn compare_recovery(replay: bool) {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let supervisor: ProcessIdentity = serde_json::from_str(get_runtime_process_instance_id().unwrap()).unwrap();
    let mut old = read_process_identity(std::process::id()).unwrap();
    old.incarnation = Some("old-incarnation".into());
    for (case, lifecycle) in ["running", "cancelling"].into_iter().enumerate() {
        let source_db = root.path().join(format!("source{case}.sqlite"));
        let native_db = JsString::from(root.path().join(format!("native{case}.sqlite")).to_str().unwrap());
        let now = pi_coding_agent::utils::iso_time::now_iso_string();
        let agent = json!({"id":"child","parentId":"main","displayName":"Child","agentType":"worker","lifecycle":lifecycle,"revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true},"worker":{"adapter":"runtime","handleId":std::process::id().to_string(),"toolCallId":"tool"}});
        let expected_owner = json!({"sessionPath":"/main.jsonl","agentId":"child","processIdentity":old,"owner":{"sessionId":"supervisor","agentId":null}});
        let input = json!({"expectedOwner":expected_owner,"nowIso":now,"supervisor":{"sessionId":"supervisor","agentId":null,"processIdentity":supervisor}});
        let mut steps = Vec::new();
        for db in [json!(source_db), json!(native_db)] {
            steps.push(json!({"op":"call","args":["registerRuntimeMailboxListener",[db,{"sessionId":"supervisor","agentId":null},std::process::id(),"/main.jsonl",{"runtimeInstanceId":get_runtime_process_instance_id().unwrap()}]]}));
            // Child creation requires running revision 1; bootstrap a cancelling snapshot only after ownership exists.
            let mut running = agent.clone();
            running["lifecycle"] = json!("running");
            steps.push(json!({"op":"call","args":["createMultiAgentChildWithRuntimeOwnership",[db,{"sessionPath":"/main.jsonl","agentId":"child","nowIso":now,"agent":running,"processIdentity":old,"owner":{"sessionId":"supervisor","agentId":null}}]]}));
            if lifecycle == "cancelling" {
                steps.push(json!({"op":"call","args":["commitMultiAgentLifecycleMutation",[db,{"sessionPath":"/main.jsonl","agentId":"child","processIdentity":old,"owner":{"sessionId":"supervisor","agentId":null},"requestedLifecycle":"cancelling","updatedAt":now}]]}));
            }
        }
        if replay {
            steps.push(json!({"op":"call","args":["recoverDeadMultiAgentRuntime",[native_db,input]]}));
        }
        steps.push(json!({"op":"call","args":["recoverDeadMultiAgentRuntime",[source_db,input]]}));
        if replay {
            steps.push(json!({"op":"call","args":["recoverDeadMultiAgentRuntime",[source_db,input]]}));
        }
        let response = source.request(&json!({"id":1,"steps":steps}));
        let expected = response["output"].as_array().unwrap().last().unwrap();
        assert_eq!(expected["ok"], true, "{response}");
        assert_eq!(expected["value"]["ok"], true, "{response}");
        let result = recover_dead_multi_agent_runtime(
            &native_db,
            &RecoverDeadMultiAgentRuntimeInput {
                expected_owner: MultiAgentRuntimeOwnershipIdentity {
                    session_path: "/main.jsonl".into(),
                    agent_id: "child".into(),
                    process_identity: old.clone(),
                    owner: MultiAgentRuntimeOwnerIdentity {
                        session_id: "supervisor".into(),
                        agent_id: None,
                    },
                },
                now_iso: now,
                supervisor: SupervisorRuntimeOwnership {
                    session_id: "supervisor".into(),
                    agent_id: None,
                    process_identity: supervisor.clone(),
                },
            },
        )
        .unwrap();
        let actual = match result {
            RecoverDeadMultiAgentRuntimeResult::Ok {
                agent,
                terminal_revision,
            } => json!({"agent":JsValue::Object(agent),"ok":true,"terminalRevision":terminal_revision}),
            RecoverDeadMultiAgentRuntimeResult::Err { error } => json!({"ok":false,"error":error.as_str()}),
        };
        assert_eq!(
            JsValue::from(actual),
            JsValue::from(expected["value"].clone()),
            "{lifecycle} replay={replay}"
        );
        assert_eq!(
            JsValue::from(json!(
                read_multi_agent_agent(&native_db, "/main.jsonl", "child").unwrap()
            )),
            JsValue::from(expected["value"]["agent"].clone())
        );
        let outbox = with_control_db(&native_db, |db| {
            Ok(
                db.query_row("SELECT COUNT(*) FROM multi_agent_terminal_outbox", [], |row| {
                    row.get::<_, i64>(0)
                })?,
            )
        })
        .unwrap();
        assert_eq!(outbox, 1, "replay must not duplicate terminal publication");
    }
}
#[test]
fn recovery_accepts_a_live_process_with_a_superseded_incarnation() {
    compare_recovery(false);
}
#[test]
fn completed_lost_runtime_recovery_replays_without_new_mutations() {
    compare_recovery(true);
}
