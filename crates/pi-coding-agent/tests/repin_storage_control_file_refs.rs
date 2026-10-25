use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{list_runtime_mailbox_messages, with_control_db};
use pi_js_compat::js_json::{self, JsString};
use serde_json::json;

#[test]
fn public_mailbox_references_use_frozen_normalized_fields_and_order() {
    let root = tempfile::tempdir().unwrap();
    let path = JsString::from(root.path().join("control.sqlite").to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let input = json!({"recipient":{"sessionId":"recipient","agentId":null},"sender":{"sessionId":"sender","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/source.jsonl","messageId":"m"},"message":{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"references","fileRefs":[{"label":"Bash output","path":"/fixture/out.log"},{"path":"/fixture/日志.txt","origin":"tool"}]}});
    let result = source.request(&json!({"id":1,"steps":[{"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[path,input]]},{"op":"callWire","args":["listRuntimeMailboxMessages",serde_json::to_string(&json!([path])).unwrap()]}]}));
    assert_eq!(result["output"][0]["ok"], true, "{result}");
    assert_eq!(result["output"][1]["ok"], true, "{result}");
    let read_canonical = || {
        with_control_db(&path, |db| {
            Ok(
                db.query_row("SELECT data FROM multi_agent_mailbox_messages", [], |row| {
                    row.get::<_, String>(0)
                })?,
            )
        })
        .unwrap()
    };
    let before = read_canonical();
    let native = list_runtime_mailbox_messages(&path).unwrap();
    let native_wire = js_json::stringify_lossless(&js_json::to_value(&native).unwrap());
    assert_eq!(native_wire, result["output"][1]["value"].as_str().unwrap());
    assert_eq!(
        read_canonical(),
        before,
        "public normalization must not rewrite canonical references"
    );
}
