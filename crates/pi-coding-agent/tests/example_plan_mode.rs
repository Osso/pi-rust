//! Known missing API proof, NOT a claim that plan-mode is ported.
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_manager::SessionManager;
use serde_json::json;

#[test]
fn restored_plan_mutates_ts_custom_entry_but_native_snapshot_cannot_write_back() {
    let initial =
        json!({"enabled":false,"executing":true,"todos":[{"step":1,"text":"Read source files","completed":false}]});
    let message = json!({"role":"assistant","content":[{"type":"text","text":"[DONE:1]"}],"api":"openai-completions","provider":"fixture","model":"faux","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":1});
    let mut ts = oracle::TsOracle::spawn(
        "extension-examples-g-oracle.ts",
        "packages/coding-agent/examples/extensions/plan-mode/index.ts",
    );
    let result = ts.request(&json!({"entries":[{"type":"custom","customType":"plan-mode","data":initial},{"type":"message","message":message}],"steps":[{"op":"event","event":{"type":"session_start","reason":"startup"}}]}));
    assert_eq!(result["entries"][0]["data"]["todos"][0]["completed"], true);
    // Native manager owns data. Restoring and updating its public snapshot does
    // not update the stored custom entry; no custom-entry mutation API exists.
    let mut native = SessionManager::in_memory(Some("/fixture"), None).unwrap();
    native.append_custom_entry("plan-mode", Some(initial)).unwrap();
    native.append_message(serde_json::from_value(message).unwrap()).unwrap();
    let mut restored = serde_json::to_value(native.get_entries()).unwrap();
    restored[0]["data"]["todos"][0]["completed"] = json!(true);
    assert_eq!(restored[0]["data"]["todos"][0]["completed"], true);
    let stored = serde_json::to_value(native.get_entries()).unwrap();
    assert_eq!(stored[0]["data"]["todos"][0]["completed"], false);
}
