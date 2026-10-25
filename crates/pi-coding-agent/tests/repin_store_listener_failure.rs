use crate::shared_oracle as oracle;

use pi_coding_agent::core::multi_agent_store::{AgentNode, MultiAgentStore, MultiAgentStoreOptions};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

fn snapshot(revision: i64, lifecycle: &str) -> Value {
    json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker",
        "lifecycle":lifecycle,"revision":revision,"createdAt":"2026-10-25T00:00:00.000Z",
        "updatedAt":"2026-10-25T00:00:00.000Z","cwd":"/fixture",
        "permission":{"policy":"on-request","narrowed":true}})
}

fn updated_lifecycle(kind: &str) -> &str {
    match kind {
        "update" => "running",
        "transition" => "waiting_for_input",
        "notification" => "completed",
        _ => panic!("unknown listener kind"),
    }
}

#[test]
fn update_transition_and_lifecycle_listener_failures_log_locally_and_continue_publication() {
    let running = snapshot(1, "running");
    let mut source = oracle::TsOracle::spawn(
        "storage-listener-oracle.mts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    for kind in ["update", "transition", "notification"] {
        let updated = snapshot(2, updated_lifecycle(kind));
        for failure in [json!("failed 漢😀"), json!(null), json!({"detail":"failure","count":2})] {
            let expected =
                source.request(&json!({"id":1,"kind":kind,"running":running,"updated":updated,"failure":failure}));
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "listener_failure_child", "--nocapture"])
                .env("PI_STORAGE_LISTENER_KIND", kind)
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
            assert_eq!(json!(logs), expected["logs"], "{kind}");
            let stdout = String::from_utf8(child.stdout).unwrap();
            let result = stdout.lines().find_map(|line| line.strip_prefix("RESULT ")).unwrap();
            let observed: Value = serde_json::from_str(result).unwrap();
            assert_eq!(observed["observed"], expected["observed"], "{kind}");
            assert_eq!(observed["agent"], expected["agent"], "{kind}");
        }
    }
}

#[test]
#[ignore = "invoked only by update_transition_and_lifecycle_listener_failures_log_locally_and_continue_publication"]
fn listener_failure_child() {
    let kind = std::env::var("PI_STORAGE_LISTENER_KIND").unwrap();
    let failure: Value = serde_json::from_str(&std::env::var("PI_STORAGE_LISTENER_FAILURE").unwrap()).unwrap();
    let store = MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-10-25T00:00:00.000Z".into()))),
    });
    let running: AgentNode = serde_json::from_value(snapshot(1, "running")).unwrap();
    let updated: AgentNode = serde_json::from_value(snapshot(2, updated_lifecycle(&kind))).unwrap();
    store.publish_lifecycle_coordinator_snapshot(&running).unwrap();
    let failure = CallbackError::from_rejection(AbortReason::Value(JsValue::from(failure)));
    let observed = Arc::new(Mutex::new(Vec::<Value>::new()));
    let recorded = Arc::clone(&observed);
    let (_failing, _continuing) = match kind.as_str() {
        "update" => (
            store.subscribe_agent_updates(move |_, _| Err(failure.clone())),
            store.subscribe_agent_updates(move |_, current| {
                recorded.lock().unwrap().push(json!(current.revision));
                Ok(())
            }),
        ),
        "transition" => (
            store.subscribe_agent_transitions(move |_, _| Err(failure.clone())),
            store.subscribe_agent_transitions(move |_, current| {
                recorded.lock().unwrap().push(json!(current.revision));
                Ok(())
            }),
        ),
        "notification" => (
            store.subscribe_lifecycle_notifications(move |_| Err(failure.clone())),
            store.subscribe_lifecycle_notifications(move |message| {
                recorded.lock().unwrap().push(json!(message));
                Ok(())
            }),
        ),
        _ => panic!("unknown listener kind"),
    };
    if kind == "notification" {
        store.publish_terminal_outbox_snapshot(&updated).unwrap();
    } else {
        store.publish_lifecycle_coordinator_snapshot(&updated).unwrap();
    }
    println!(
        "RESULT {}",
        json!({"observed":*observed.lock().unwrap(),"agent":store.get_agent(&"child".into())})
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
