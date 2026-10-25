use super::repin_storage_history as oracle;
use pi_coding_agent::core::multi_agent_store::AgentSnapshot;
use pi_coding_agent::core::session_control_db::{
    CreateMultiAgentAttachmentInput, CreateMultiAgentAttachmentResult, create_multi_agent_attachment, with_control_db,
};
use pi_js_compat::js_json::{JsString, JsValue, from_value};
use serde_json::json;

#[test]
fn control_attachment_preserves_input_json_without_store_copy_normalization() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    for (case, field, value) in [
        (
            0,
            "account",
            json!({"id":"account","budgetId":"budget","concurrencyCap":2}),
        ),
        (
            1,
            "result",
            json!({"summary":"done","fileRefs":[{"label":"result","path":"/result"}],"durationMs":2}),
        ),
        (
            2,
            "worker",
            json!({"handleId":"handle","adapter":"runtime","cwd":"/worker"}),
        ),
        (3, "transcript", json!({"sessionId":"child","path":"/child.jsonl"})),
        (
            4,
            "eventStream",
            json!({"path":"/events","eventCount":2,"truncated":false,"byteLimit":3}),
        ),
    ] {
        let source_db = root.path().join(format!("source{case}.sqlite"));
        let native_db = JsString::from(root.path().join(format!("native{case}.sqlite")).to_str().unwrap());
        let mut agent = json!({"id":"child","parentId":"main","displayName":"Child","agentType":"worker","origin":"attached","lifecycle":"waiting_for_input","revision":1,"createdAt":"2026-10-25T00:00:00.000Z","updatedAt":"2026-10-25T00:00:00.000Z","cwd":"/fixture","permission":{"narrowed":true,"policy":"on-request"}});
        agent[field] = value;
        let response=source.request(&json!({"id":1,"steps":[
            {"op":"call","args":["createMultiAgentAttachment",[source_db,{"agent":agent,"agentId":"child","nowIso":"2026-10-25T00:00:00.000Z","sessionPath":"/main.jsonl"}]]},
            {"op":"fixtureSql","args":[source_db,"SELECT data FROM multi_agent_agents WHERE agent_id='child'"]}
        ]}));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["output"][0]["value"]["ok"], true, "{response}");
        let expected = response["output"][1]["value"][0]["data"].as_str().unwrap();
        let agent: AgentSnapshot = from_value(&JsValue::from(agent)).unwrap();
        let result = create_multi_agent_attachment(
            &native_db,
            &CreateMultiAgentAttachmentInput {
                agent,
                agent_id: "child".into(),
                now_iso: "2026-10-25T00:00:00.000Z".into(),
                session_path: "/main.jsonl".into(),
            },
        )
        .unwrap();
        assert!(matches!(result, CreateMultiAgentAttachmentResult::Ok { .. }));
        let stored = with_control_db(&native_db, |db| {
            Ok(db.query_row(
                "SELECT data FROM multi_agent_agents WHERE agent_id='child'",
                [],
                |row| row.get::<_, String>(0),
            )?)
        })
        .unwrap();
        assert_eq!(stored, expected, "{field}");
    }
}
