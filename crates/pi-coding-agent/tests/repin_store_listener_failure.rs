#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::multi_agent_store::{AgentNode, MultiAgentStore, MultiAgentStoreOptions};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

fn snapshot(revision: i64) -> Value {
    json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker",
        "lifecycle":"running","revision":revision,"createdAt":"2026-10-25T00:00:00.000Z",
        "updatedAt":"2026-10-25T00:00:00.000Z","cwd":"/fixture",
        "permission":{"policy":"on-request","narrowed":true}})
}

#[test]
fn update_listener_failures_log_locally_and_continue_publication() {
    let running = snapshot(1);
    let updated = snapshot(2);
    let mut source = oracle::TsOracle::spawn(
        "storage-listener-oracle.mts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    for failure in [
        json!("failed 漢😀"),
        json!(null),
        json!({"detail":"failure", "count":2}),
    ] {
        let expected = source.request(&json!({"id":1,"running":running,"updated":updated,"failure":failure}));
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "listener_failure_child", "--nocapture"])
            .env("PI_STORAGE_LISTENER_FAILURE", failure.to_string())
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8(child.stderr.clone()).unwrap()
        );
        let stderr = String::from_utf8(child.stderr).unwrap();
        let logs: Vec<_> = stderr
            .lines()
            .filter(|line| line.starts_with("MultiAgentStore "))
            .collect();
        assert_eq!(json!(logs), expected["logs"]);
        let stdout = String::from_utf8(child.stdout).unwrap();
        let result = stdout.lines().find_map(|line| line.strip_prefix("RESULT ")).unwrap();
        let observed: Value = serde_json::from_str(result).unwrap();
        assert_eq!(observed["revisions"], expected["revisions"]);
        assert_eq!(observed["agent"], expected["agent"]);
    }
}

#[test]
#[ignore = "invoked only by update_listener_failures_log_locally_and_continue_publication"]
fn listener_failure_child() {
    let failure: Value = serde_json::from_str(&std::env::var("PI_STORAGE_LISTENER_FAILURE").unwrap()).unwrap();
    let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
    let running: AgentNode = serde_json::from_value(snapshot(1)).unwrap();
    let updated: AgentNode = serde_json::from_value(snapshot(2)).unwrap();
    store.publish_lifecycle_coordinator_snapshot(&running).unwrap();
    let failure = CallbackError::from_rejection(AbortReason::Value(JsValue::from(failure)));
    let _failing = store.subscribe_agent_updates(move |_, _| Err(failure.clone()));
    let revisions = Arc::new(Mutex::new(Vec::new()));
    let recorded = revisions.clone();
    let _continuing = store.subscribe_agent_updates(move |_, current| {
        recorded.lock().unwrap().push(current.revision);
        Ok(())
    });
    store.publish_lifecycle_coordinator_snapshot(&updated).unwrap();
    println!(
        "RESULT {}",
        json!({"revisions":*revisions.lock().unwrap(),"agent":store.get_agent(&"child".into())})
    );
}
