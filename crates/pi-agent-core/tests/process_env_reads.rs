//! `~` expansion observes runtime `process.env` overlay mutations like Node.

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::FileSystem;
use pi_js_compat::js_json::JsString;
use pi_js_compat::process_env;

#[tokio::test]
async fn home_expansion_observes_process_env_overlay() {
    process_env::set("HOME", "/overlay/home");
    let filesystem = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: "/work".into(),
        ..Default::default()
    });
    let resolved = filesystem
        .absolute_path(&JsString::from("~/session.jsonl"), None)
        .await
        .expect("absolute path");
    process_env::remove("HOME");

    assert_eq!(resolved, "/overlay/home/session.jsonl");
}
