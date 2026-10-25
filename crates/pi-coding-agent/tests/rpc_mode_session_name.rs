//! RPC `set_session_name` writes control-db metadata only
//! (`test/rpc.test.ts` name assertions, approved v15 deviation 2026-10-20).

#[path = "support/mode_harness.rs"]
mod mode_harness;

use pi_js_compat::js_json::JsString;
use std::path::Path;

use mode_harness::ModeHarness;
use pi_coding_agent::core::session_control_db::{get_control_db_path, read_session_metadata};
use serde_json::{Value, json};

const RESPONSES: &str = r#"[{"text":"Hello there","endTurn":"greeted","toolId":"call-1"}]"#;

#[test]
fn set_session_name_persists_metadata_without_jsonl_session_info() {
    let root = tempfile::tempdir().expect("temp dir");
    let cwd = root.path().join("cwd");
    let agent_dir = root.path().join("agent");
    let sessions = root.path().join("sessions");
    let state_dir = root.path().join("state");
    for directory in [&cwd, &agent_dir, &sessions, &state_dir] {
        std::fs::create_dir_all(directory).expect("fixture directory");
    }
    let cwd_string = cwd.display().to_string();
    let env = vec![
        ("PI_HARNESS_MODE", "rpc".to_owned()),
        ("PI_HARNESS_CWD", cwd_string.clone()),
        ("PI_HARNESS_RESPONSES", RESPONSES.to_owned()),
        ("PI_HARNESS_AGENT_DIR", agent_dir.display().to_string()),
        ("PI_HARNESS_SESSION_DIR", sessions.display().to_string()),
        ("PI_CODING_AGENT_STATE_DIR", state_dir.display().to_string()),
    ];
    let mut harness = ModeHarness::spawn_rust(&env, Path::new(&cwd_string));

    harness.send(&json!({"id": "p1", "type": "prompt", "message": "hello"}).to_string());
    let _ = harness.wait_for_id("p1");
    let _ = harness.wait_for(|line| line.contains("\"type\":\"agent_end\""));

    harness.send(&json!({"id": "n1", "type": "set_session_name", "name": "my-test-session"}).to_string());
    let response = harness.wait_for_id("n1");
    assert_eq!(
        serde_json::from_str::<Value>(&response).expect("response")["success"],
        json!(true)
    );

    harness.send(&json!({"id": "s1", "type": "get_state"}).to_string());
    let state = harness.wait_for_id("s1");
    let state: Value = serde_json::from_str(&state).expect("state response");
    assert_eq!(state["data"]["sessionName"], json!("my-test-session"));
    let session_file = state["data"]["sessionFile"].as_str().expect("session file").to_owned();

    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path(state_dir.as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    assert_eq!(
        read_session_metadata(&control_db_path, &session_file)
            .expect("metadata")
            .expect("session metadata")
            .name
            .as_ref(),
        Some(&JsString::from("my-test-session"))
    );
    let entry_types: Vec<String> = std::fs::read_to_string(&session_file)
        .expect("session file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line).expect("jsonl line")["type"]
                .as_str()
                .expect("entry type")
                .to_owned()
        })
        .collect();
    assert!(
        !entry_types.iter().any(|kind| kind == "session_info"),
        "RPC name changes never append session_info entries: {entry_types:?}"
    );

    let (_, code, stderr) = harness.finish();
    assert_eq!(code, 0, "stderr: {stderr}");
}
