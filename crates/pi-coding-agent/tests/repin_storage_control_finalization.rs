use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{
    FinalizeDetachedJobInput, FinalizeDetachedJobResult, finalize_detached_job, with_control_db,
};
use pi_js_compat::js_json::{JsString, JsValue, from_value};
use serde_json::json;

#[test]
fn detached_finalization_uses_distinct_new_and_existing_reference_orders_and_replays_cross_runtime() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for (case, kind) in ["completed", "failed", "aborted"].into_iter().enumerate() {
        let source_db = root.path().join(format!("source{case}.sqlite"));
        let native_db = JsString::from(root.path().join(format!("native{case}.sqlite")).to_str().unwrap());
        let now = pi_coding_agent::utils::iso_time::now_iso_string();
        let identity = pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).unwrap();
        let agent = json!({"id":"job","parentId":"main","displayName":"Job","agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true},"worker":{"adapter":"runtime","handleId":std::process::id().to_string()},"result":{"fileRefs":[{"label":"prior","path":"/prior"},{"path":"/output","label":"Bash output"}]}});
        let outcome = match kind {
            "completed" => json!({"kind":"completed","exitCode":0,"summary":"done"}),
            "failed" => json!({"kind":"failed","error":{"message":"failed","code":"FIXTURE"}}),
            "aborted" => json!({"kind":"aborted","reason":"stop"}),
            _ => unreachable!(),
        };
        let terminal = json!({"jobId":"job","owner":{"sessionId":"supervisor","agentId":null},"processIdentity":identity,"outputLabel":"Bash output","terminalAt":now,"toolCallId":"tool","durationMs":12,"outcome":outcome,"output":{"label":"Bash output","path":"/output","size":8,"sha256":"abc"}});
        let seed = |db: serde_json::Value| json!({"op":"call","args":["createMultiAgentChildWithRuntimeOwnership",[db,{"sessionPath":"/main.jsonl","agentId":"job","nowIso":now,"agent":agent,"processIdentity":identity,"owner":{"sessionId":"supervisor","agentId":null}}]]});
        let input = json!({"sessionPath":"/main.jsonl","terminal":terminal});
        let response=source.request(&json!({"id":1,"steps":[seed(json!(source_db)),seed(json!(native_db)),{"op":"call","args":["finalizeDetachedJob",[source_db,input]]},{"op":"fixtureSql","args":[source_db,"SELECT data FROM multi_agent_agents WHERE agent_id='job'"]}]}));
        assert_eq!(response["output"][2]["value"]["ok"], true, "{response}");
        let input = FinalizeDetachedJobInput {
            session_path: "/main.jsonl".into(),
            terminal: from_value(&JsValue::from(terminal.clone())).unwrap(),
        };
        let result = finalize_detached_job(&native_db, &input).unwrap();
        assert!(matches!(result, FinalizeDetachedJobResult::Ok { .. }), "{result:?}");
        let data = with_control_db(&native_db, |db| {
            Ok(
                db.query_row("SELECT data FROM multi_agent_agents WHERE agent_id='job'", [], |row| {
                    row.get::<_, String>(0)
                })?,
            )
        })
        .unwrap();
        assert_eq!(json!(data), response["output"][3]["value"][0]["data"], "{kind}");
        let native_replay = finalize_detached_job(source_db.to_str().unwrap(), &input).unwrap();
        assert!(
            matches!(native_replay, FinalizeDetachedJobResult::Ok { .. }),
            "{native_replay:?}"
        );
        let replay=source.request(&json!({"id":2,"steps":[{"op":"call","args":["finalizeDetachedJob",[native_db,{"sessionPath":"/main.jsonl","terminal":terminal}]]}]}));
        assert_eq!(replay["output"][0]["value"]["ok"], true, "{replay}");
    }
}
