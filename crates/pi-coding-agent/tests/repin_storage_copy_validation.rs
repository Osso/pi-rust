use super::repin_storage_history as oracle;
use pi_coding_agent::core::multi_agent_store::{
    AgentMailboxMessage, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions,
};
use pi_js_compat::js_json::{JsValue, from_value};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn snapshot() -> Value {
    json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":"2026-10-25T00:00:00.000Z","updatedAt":"2026-10-25T00:00:00.000Z","cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}})
}
fn compare(source: &mut oracle::TsOracle, op: &str, agent: Value, refs: Value) {
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let initial = snapshot();
    let message = json!({"id":"message_1","fromAgentId":"main","toAgentId":"child","kind":"steer","status":"pending","createdAt":now,"updatedAt":now,"fileRefs":refs});
    let expected = source.request(&json!({"id":1,"op":op,"initial":initial,"agent":agent,"message":message,"now":now}));
    let calls = Arc::new(AtomicUsize::new(0));
    let called = Arc::clone(&calls);
    let store = MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(move || {
            called.fetch_add(1, Ordering::SeqCst);
            Ok(now.clone().into())
        })),
    });
    let initial: AgentSnapshot = from_value(&JsValue::from(initial)).unwrap();
    store.publish_lifecycle_coordinator_snapshot(&initial).unwrap();
    let agent: AgentSnapshot = from_value(&JsValue::from(agent)).unwrap();
    let message: AgentMailboxMessage = from_value(&JsValue::from(message)).unwrap();
    let outcome = match op {
        "snapshot" => store.publish_lifecycle_coordinator_snapshot(&agent),
        "terminal" => store.publish_terminal_outbox_snapshot(&agent),
        "steering" => store.publish_lifecycle_coordinator_steering(&agent, &message),
        _ => panic!("unknown publication"),
    };
    let result = match outcome {
        Ok(()) => json!({"ok":true}),
        Err(error) => json!({"ok":false,"error":{"name":"Error","message":error.message()}}),
    };
    let samples = calls.load(Ordering::SeqCst);
    let actual = json!({"id":1,"result":result,"samples":samples,"agents":store.list_agents(),"messages":store.list_mailbox_messages().unwrap()});
    assert_eq!(JsValue::from(actual), JsValue::from(expected), "{op}");
}

#[test]
fn snapshot_copy_rejects_relative_nested_paths_before_state_or_clock_changes() {
    let mut source = oracle::TsOracle::spawn(
        "storage-store-copy-oracle.mts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    for op in ["snapshot", "terminal", "steering"] {
        for (key, value) in [
            (
                "worker",
                json!({"adapter":"runtime","handleId":"handle","cwd":"relative/漢"}),
            ),
            ("transcript", json!({"sessionId":"session","path":"relative/漢"})),
            (
                "eventStream",
                json!({"eventCount":0,"truncated":false,"path":"relative/漢"}),
            ),
            ("worktree", json!({"path":"relative/漢"})),
            ("result", json!({"fileRefs":[{"path":"relative/漢","label":"result"}]})),
        ] {
            let mut agent = snapshot();
            agent["revision"] = json!(2);
            agent[key] = value;
            compare(&mut source, op, agent, json!([]));
        }
        let mut agent = snapshot();
        agent["result"] = json!({"fileRefs":[{"path":"relative/result"}]});
        agent["worker"] = json!({"adapter":"runtime","handleId":"h","cwd":"relative/worker"});
        agent["eventStream"] = json!({"path":"relative/event","eventCount":0,"truncated":false});
        compare(&mut source, op, agent, json!([]));
    }
}

#[test]
fn steering_copy_validates_and_canonicalizes_file_reference_objects() {
    let mut source = oracle::TsOracle::spawn(
        "storage-store-copy-oracle.mts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    for refs in [
        json!([{"path":"relative/漢","label":"file"}]),
        json!([{"path":"/valid/漢","label":"file","extra":"drop"}]),
    ] {
        let mut agent = snapshot();
        agent["revision"] = json!(2);
        compare(&mut source, "steering", agent, refs);
    }
}
