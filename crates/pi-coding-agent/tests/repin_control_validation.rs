use pi_js_compat::js_json::JsString;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;
fn node(id: &str) -> Value {
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    json!({"id":id,"displayName":id,"agentType":"worker","lifecycle":"completed","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}})
}
#[test]
fn bootstrap_rejects_nonboolean_notification_policy_before_writing() {
    let t = tempfile::tempdir().unwrap();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for (index, value) in [Value::Null, json!("true"), json!(1), json!([]), json!({})]
        .into_iter()
        .enumerate()
    {
        let db = t
            .path()
            .join(format!("case{index}.sqlite"))
            .to_str()
            .expect("utf-8 test path")
            .to_owned();
        let mut data = node("bad");
        data["suppressTerminalNotification"] = value;
        let peer = ts.request(
            &json!({"id":1,"steps":[{"op":"call","args":["bootstrapMultiAgentAgent",[db,"/fixture/s","bad",data]]}]}),
        );
        assert_eq!(peer["output"][0]["ok"], json!(false), "{peer:?}");
        let native = bootstrap_multi_agent_agent(&db, "/fixture/s", &JsString::from("bad"), &data);
        assert!(native.is_err(), "Rust accepted source-rejected policy: {data:?}");
        assert_eq!(
            native.unwrap_err().message(),
            peer["output"][0]["error"]["message"].as_string().unwrap()
        );
        assert!(!std::path::Path::new(&db).exists(), "validation must precede DB setup");
    }
}
#[test]
fn persisted_nonboolean_policy_is_not_hidden_by_typed_input_exclusions() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let response=ts.request(&json!({"id":1,"steps":[{"op":"call","args":["bootstrapMultiAgentAgent",[db,"/fixture/s","bad",node("bad")]]},{"op":"fixtureSql","args":[db,"UPDATE multi_agent_agents SET data=json_set(data,'$.suppressTerminalNotification',42) RETURNING agent_id",[]]},{"op":"call","args":["readMultiAgentState",[db,"/fixture/s"]]}]}));
    assert_eq!(response["output"][0]["ok"], json!(true));
    assert_eq!(response["output"][2]["ok"], json!(false));
    let native = read_multi_agent_state(&db, "/fixture/s");
    assert!(
        native.is_err(),
        "stored invalid policy must be rejected at restore boundary"
    );
    assert_eq!(
        native.unwrap_err().message(),
        response["output"][2]["error"]["message"].as_string().unwrap()
    );
}
