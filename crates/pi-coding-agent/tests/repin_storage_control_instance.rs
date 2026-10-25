use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::get_runtime_process_instance_id;
use serde_json::{Value, json};

fn assert_instance(wire: &str) -> Value {
    let value: Value = serde_json::from_str(wire).unwrap();
    let object = value.as_object().unwrap();
    assert_eq!(
        object.keys().map(String::as_str).collect::<Vec<_>>(),
        ["pid", "startTimeTicks", "incarnation"]
    );
    assert!(value["pid"].as_u64().is_some());
    assert!(value["startTimeTicks"].as_u64().is_some());
    let incarnation = value["incarnation"].as_str().unwrap();
    assert_eq!(incarnation.len(), 36);
    assert_eq!(
        &incarnation[14..15],
        "4",
        "runtime incarnation must use crypto.randomUUID's UUIDv4 format: {wire}"
    );
    assert!(matches!(&incarnation[19..20], "8" | "9" | "a" | "b"));
    value
}

#[test]
fn process_instance_wire_uses_a_cached_v4_incarnation_like_frozen() {
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let result = source.request(&json!({"id":1,"steps":[{"op":"call","args":["getRuntimeProcessInstanceId",[]]},{"op":"call","args":["getRuntimeProcessInstanceId",[]]}]}));
    assert_eq!(result["output"][0]["ok"], true, "{result}");
    assert_eq!(result["output"][0], result["output"][1]);
    assert_instance(result["output"][0]["value"].as_str().unwrap());
    let wire = get_runtime_process_instance_id().unwrap();
    let native = assert_instance(wire);
    assert_eq!(native["pid"].as_u64(), Some(u64::from(std::process::id())));
    assert_eq!(wire, get_runtime_process_instance_id().unwrap());
}
