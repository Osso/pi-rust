use super::repin_storage_history as oracle;
use pi_coding_agent::core::multi_agent_store::AgentCurrentActivityOwner;
use pi_coding_agent::core::session_control_db::{
    update_multi_agent_agent_activity, update_multi_agent_agent_current_activity, with_control_db,
};
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::json;

fn compare_metadata_update(clear: bool) {
    let root = tempfile::tempdir().unwrap();
    let source_db = root.path().join("source.sqlite");
    let native_db = JsString::from(root.path().join("native.sqlite").to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let identity = pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).unwrap();
    let mut agent = json!({"id":"child","parentId":"main","displayName":"Child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}});
    if clear {
        agent["lastActivity"] = json!({"description":"old activity"});
    }
    let activity = json!({"phase":"thinking","startedAt":now});
    let model = json!({"providerId":"fixture","modelId":"new"});
    let ownership = json!({"ownerSessionId":"supervisor","processIdentity":identity});
    let setup = |db: serde_json::Value| {
        if clear {
            json!({"op":"call","args":["bootstrapMultiAgentAgent",[db,"/main.jsonl","child",agent]]})
        } else {
            json!({"op":"call","args":["createMultiAgentChildWithRuntimeOwnership",[db,{"sessionPath":"/main.jsonl","agentId":"child","nowIso":now,"agent":agent,"processIdentity":identity,"owner":{"sessionId":"supervisor","agentId":null}}]]})
        }
    };
    let update = if clear {
        json!({"op":"clearAgentActivity","args":[source_db,"/main.jsonl","child",now]})
    } else {
        json!({"op":"call","args":["updateMultiAgentAgentCurrentActivity",[source_db,"/main.jsonl","child",activity,now,ownership,model]]})
    };
    let response=source.request(&json!({"id":1,"steps":[setup(json!(source_db)),setup(json!(native_db)),update,{"op":"fixtureSql","args":[source_db,"SELECT data FROM multi_agent_agents WHERE agent_id='child'"]}]}));
    assert_eq!(response["ok"], true, "{response}");
    let actual = if clear {
        update_multi_agent_agent_activity(&native_db, "/main.jsonl", &"child".into(), None, now)
    } else {
        update_multi_agent_agent_current_activity(
            &native_db,
            "/main.jsonl",
            &"child".into(),
            Some(&JsValue::from(activity)),
            now,
            &AgentCurrentActivityOwner {
                owner_session_id: "supervisor".into(),
                process_identity: identity,
            },
            Some(&JsValue::from(model)),
        )
    }
    .unwrap();
    assert_eq!(
        JsValue::from(json!(actual)),
        JsValue::from(response["output"][2]["value"].clone())
    );
    let stored = with_control_db(&native_db, |db| {
        Ok(db.query_row(
            "SELECT data FROM multi_agent_agents WHERE agent_id='child'",
            [],
            |row| row.get::<_, String>(0),
        )?)
    })
    .unwrap();
    assert_eq!(json!(stored), response["output"][3]["value"][0]["data"]);
}
#[test]
fn clearing_last_activity_removes_the_previous_value() {
    compare_metadata_update(true);
}
#[test]
fn combined_activity_and_model_updates_keep_frozen_field_order() {
    compare_metadata_update(false);
}
