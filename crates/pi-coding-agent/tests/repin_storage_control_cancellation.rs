use super::repin_storage_history as oracle;
use pi_coding_agent::core::multi_agent_store::AgentSnapshot;
use pi_coding_agent::core::runtime_process::read_process_identity;
use pi_coding_agent::core::session_control_db::{
    CommitMultiAgentLifecycleMutationInput, CommitMultiAgentLifecycleMutationResult,
    CreateMultiAgentChildWithRuntimeOwnershipInput, DetachedCancellation, MultiAgentRuntimeOwnerIdentity,
    commit_multi_agent_lifecycle_mutation, create_multi_agent_child_with_runtime_ownership, read_multi_agent_agent,
    with_control_db,
};
use pi_js_compat::js_json::{JsString, JsValue, from_value};
use serde_json::json;

#[test]
fn expired_detached_cancellation_rolls_back_lifecycle_and_command_together() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for (case, updated_at) in [
        "2020-01-01T00:00:00.000Z".to_owned(),
        pi_coding_agent::utils::iso_time::now_iso_string(),
    ]
    .into_iter()
    .enumerate()
    {
        let source_db = root.path().join(format!("source{case}.sqlite"));
        let native_db = JsString::from(root.path().join(format!("native{case}.sqlite")).to_str().unwrap());
        let identity = read_process_identity(std::process::id()).unwrap();
        let owner = json!({"sessionId":"supervisor","agentId":null});
        let now = pi_coding_agent::utils::iso_time::now_iso_string();
        let agent = json!({"id":"child","parentId":"main","displayName":"Child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}});
        let response=source.request(&json!({"id":1,"steps":[
            {"op":"call","args":["createMultiAgentChildWithRuntimeOwnership",[source_db,{"agent":agent,"agentId":"child","nowIso":now,"sessionPath":"/main.jsonl","processIdentity":identity,"owner":owner}]]},
            {"op":"call","args":["commitMultiAgentLifecycleMutation",[source_db,{"sessionPath":"/main.jsonl","agentId":"child","processIdentity":identity,"owner":owner,"requestedLifecycle":"cancelling","updatedAt":updated_at,"detachedCancellation":{"outputLabel":"Bash output","reason":"stop"}}]]},
            {"op":"call","args":["readMultiAgentAgent",[source_db,"/main.jsonl","child"]]},
            {"op":"fixtureSql","args":[source_db,"SELECT data FROM multi_agent_mailbox_messages"]}
        ]}));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["output"][0]["value"]["ok"], true, "{response}");
        let agent: AgentSnapshot = from_value(&JsValue::from(agent)).unwrap();
        let owner = MultiAgentRuntimeOwnerIdentity {
            session_id: "supervisor".into(),
            agent_id: None,
        };
        create_multi_agent_child_with_runtime_ownership(
            &native_db,
            &CreateMultiAgentChildWithRuntimeOwnershipInput {
                agent,
                agent_id: "child".into(),
                now_iso: now,
                session_path: "/main.jsonl".into(),
                process_identity: identity.clone(),
                owner: owner.clone(),
            },
        )
        .unwrap();
        let result = commit_multi_agent_lifecycle_mutation(
            &native_db,
            &CommitMultiAgentLifecycleMutationInput {
                session_path: "/main.jsonl".into(),
                agent_id: "child".into(),
                process_identity: identity,
                owner,
                requested_lifecycle: "cancelling".into(),
                updated_at,
                detached_cancellation: Some(DetachedCancellation {
                    output_label: "Bash output".into(),
                    reason: Some("stop".into()),
                    suppress_terminal_notification: None,
                }),
            },
        );
        let outcome = match result {
            Ok(CommitMultiAgentLifecycleMutationResult::Ok { agent }) => {
                json!({"ok":true,"value":{"agent":JsValue::Object(agent),"ok":true}})
            }
            Ok(CommitMultiAgentLifecycleMutationResult::Err { error }) => {
                panic!("unexpected lifecycle rejection: {error:?}")
            }
            Err(error) => json!({"ok":false,"error":{"name":"Error","message":error.message()}}),
        };
        let agent = read_multi_agent_agent(&native_db, "/main.jsonl", "child").unwrap();
        let rows = with_control_db(&native_db, |db| {
            let mut statement = db.prepare("SELECT data FROM multi_agent_mailbox_messages")?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows.into_iter().map(|data| json!({"data":data})).collect::<Vec<_>>())
        })
        .unwrap();
        let actual = json!([outcome,{"ok":true,"value":agent},{"ok":true,"value":rows}]);
        let expected = json!([response["output"][1], response["output"][2], response["output"][3]]);
        assert_eq!(JsValue::from(actual), JsValue::from(expected));
    }
}
