#[path = "support/mode_harness.rs"]
mod mode_harness;
use crate::shared_oracle as oracle;

use mode_harness::{ModeHarness, create_package_dir, read_source_lock, verify_source_head};
use pi_agent_core::agent::AgentMessage;
use pi_ai::types::AssistantMessage;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

const IMAGE: &str = r#"{"before":{"\ud800":["\udfff",null]},"type":"image","middle":17,"data":"AA==","mimeType":"image/png","after":[{"nested":"\ud800"}]}"#;

fn message() -> JsValue {
    js_json::parse_lossless(format!(r#"{{"role":"user","content":[{IMAGE}],"timestamp":1}}"#)).unwrap()
}

fn image_wire(message: &JsValue) -> String {
    js_json::stringify_lossless(&message["content"][0])
}

#[test]
fn image_properties_cross_session_append_and_load_in_both_directions() {
    // Frozen session-manager.ts appendMessage stores the message unchanged.
    let root = tempfile::tempdir().unwrap();
    let directory = JsString::from(root.path().to_str().unwrap());
    let mut source = oracle::TsOracle::spawn(
        "session-manager-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let assistant = AssistantMessage::empty("openai-completions", "openai", "fixture", 2.0);
    let created = source.request(&js_json!({"operation":"write_session","id":"image-session","cwd":directory,"dir":directory,"messages":[message(),assistant]}));
    assert_eq!(created["ok"], JsValue::Bool(true), "{created:?}");
    let path = created["output"]["path"].as_string().unwrap();
    let mut native = SessionManager::open(path, Some(&directory), None).unwrap();
    assert_eq!(
        image_wire(&js_json::to_value(&native.build_session_context().messages[0]).unwrap()),
        IMAGE
    );
    let appended: AgentMessage = js_json::from_value(&message()).unwrap();
    native.append_message(appended).unwrap();
    drop(native);
    let reread = source.request(&js_json!({"operation":"read_session","path":path,"sessionDir":directory}));
    assert_eq!(reread["ok"], JsValue::Bool(true), "{reread:?}");
    assert_eq!(image_wire(&reread["output"]["context"]["messages"][2]), IMAGE);
    let source_append = source
        .request(&js_json!({"operation":"append_message","path":path,"sessionDir":directory,"message":message()}));
    assert_eq!(source_append["ok"], JsValue::Bool(true), "{source_append:?}");
    let reopened = SessionManager::open(path, Some(&directory), None).unwrap();
    let messages = reopened.build_session_context().messages;
    assert_eq!(messages.len(), 4);
    for index in [0, 2, 3] {
        assert_eq!(image_wire(&js_json::to_value(&messages[index]).unwrap()), IMAGE);
    }
}

fn spawn_rpc_pair(root: &std::path::Path) -> [ModeHarness; 2] {
    let lock = read_source_lock();
    verify_source_head(&lock);
    let package = create_package_dir(lock.worktree.join("packages/coding-agent").display().to_string().len());
    let start = |side: &str| {
        let base = root.join(side);
        for name in ["agent", "sessions", "state"] {
            std::fs::create_dir_all(base.join(name)).unwrap();
        }
        let env = vec![
            ("PI_HARNESS_MODE", "rpc".to_owned()),
            ("PI_HARNESS_CWD", root.display().to_string()),
            ("PI_HARNESS_RESPONSES", r#"[{"text":"ok"}]"#.to_owned()),
            ("PI_HARNESS_AGENT_DIR", base.join("agent").display().to_string()),
            ("PI_HARNESS_SESSION_DIR", base.join("sessions").display().to_string()),
            ("PI_CODING_AGENT_STATE_DIR", base.join("state").display().to_string()),
            ("PI_HARNESS_RPC_ROOT", base.join("state").display().to_string()),
            ("PI_PACKAGE_DIR", package.display().to_string()),
        ];
        if side == "source" {
            ModeHarness::spawn_typescript(&lock, "packages/coding-agent/src/modes/rpc/rpc-mode.ts", &env, root)
        } else {
            ModeHarness::spawn_rust(&env, root)
        }
    };
    [start("source"), start("native")]
}

#[test]
fn rpc_prompt_get_messages_keeps_image_unknown_values_and_order() {
    // Frozen rpc-mode.ts prompt forwards images; get_messages returns session messages.
    let root = tempfile::tempdir().unwrap();
    let pair = spawn_rpc_pair(root.path());
    for mut rpc in pair {
        rpc.send(&format!(
            r#"{{"id":"image","type":"prompt","message":"look","images":[{IMAGE}]}}"#
        ));
        let response = js_json::parse_lossless(rpc.wait_for_id("image")).unwrap();
        assert_eq!(response["success"], JsValue::Bool(true), "{response:?}");
        rpc.wait_for(|line| line.contains("\"type\":\"agent_end\""));
        rpc.send(r#"{"id":"messages","type":"get_messages"}"#);
        let response = js_json::parse_lossless(rpc.wait_for_id("messages")).unwrap();
        assert_eq!(response["success"], JsValue::Bool(true), "{response:?}");
        let content = response["data"]["messages"][0]["content"].as_array().unwrap();
        let image = content
            .iter()
            .find(|block| block["type"] == JsValue::String("image".into()))
            .unwrap();
        assert_eq!(js_json::stringify_lossless(image), IMAGE);
        let (_, code, stderr) = rpc.finish();
        assert_eq!(code, 0, "{stderr}");
    }
}
