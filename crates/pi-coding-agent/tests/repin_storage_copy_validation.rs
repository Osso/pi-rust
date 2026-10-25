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
    let actual = json!({"id":1,"result":result,"samples":samples,"agents":store.list_agents(),"agentsWire":serde_json::to_string(&store.list_agents()).unwrap(),"messages":store.list_mailbox_messages().unwrap()});
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

#[test]
fn copied_snapshot_json_keeps_nested_constructor_field_order() {
    let mut source = oracle::TsOracle::spawn(
        "storage-store-copy-oracle.mts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    for (field, value) in [
        (
            "account",
            json!({"id":"account","budgetId":"budget","concurrencyCap":2,"providerFallback":["p"],"rateLimit":{"perMinute":3},"tokenBudget":{"limit":10}}),
        ),
        (
            "worker",
            json!({"handleId":"handle","adapter":"runtime","cwd":"/worker"}),
        ),
        ("transcript", json!({"sessionId":"session","path":"/transcript"})),
        (
            "eventStream",
            json!({"path":"/events","eventCount":2,"truncated":false,"byteLimit":10}),
        ),
        (
            "result",
            json!({"summary":"done","toolCallId":"tool","fileRefs":[{"label":"result","path":"/result"}],"durationMs":12}),
        ),
        ("permission", json!({"narrowed":true,"policy":"on-request"})),
        ("worktree", json!({"base":"main","path":"/tree","branch":"feature"})),
        ("model", json!({"modelId":"m","providerId":"p"})),
        ("lastActivity", json!({"toolName":"read","description":"reading"})),
        (
            "currentActivity",
            json!({"toolName":"read","toolCallId":"tool","detail":"/file","phase":"tool","startedAt":"2026-10-25T00:00:00.000Z"}),
        ),
        ("slot", json!({"pinned":true,"index":1})),
        ("error", json!({"code":"FIXTURE","message":"failure"})),
    ] {
        let mut agent = snapshot();
        agent[field] = value;
        compare(&mut source, "snapshot", agent, json!([]));
    }
    let mut properties = serde_json::Map::new();
    properties.insert("note".into(), json!({"second":2,"first":1}));
    properties.extend(snapshot().as_object().unwrap().clone());
    compare(&mut source, "snapshot", Value::Object(properties), json!([]));
}
