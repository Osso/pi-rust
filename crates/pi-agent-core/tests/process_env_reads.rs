//! Frozen NodeExecutionEnv resolves `~` literally; HOME does not expand filesystem paths.

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::FileSystem;
use pi_js_compat::js_json::JsString;
use pi_js_compat::process_env;

#[tokio::test]
async fn home_path_remains_literal_under_process_env_overlay() {
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

    // Frozen harness/env/nodejs.ts:47–49,257–258 only applies node:path.resolve.
    // frozen_home_path_resolution_is_literal_for_each_home_value proves both HOME variants.
    assert_eq!(resolved, "/work/~/session.jsonl");
}

#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;
#[test]
fn frozen_home_path_resolution_is_literal_for_each_home_value() {
    let mut oracle = oracle::TsOracle::spawn(
        "stale-expectations-oracle.ts",
        "packages/agent-core/src/harness/messages.ts",
    );
    let observed = oracle.request(&serde_json::json!({"kind":"home","homes":["/overlay/home","/different/home"]}));
    eprintln!("frozen home proof: {observed}");
    for (index, home) in ["/overlay/home", "/different/home"].into_iter().enumerate() {
        assert_eq!(
            observed[index],
            serde_json::json!({"home":home,"resolved":{"ok":true,"value":"/work/~/session.jsonl"}})
        );
    }
}
