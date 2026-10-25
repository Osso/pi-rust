use super::repin_storage_history as oracle;
use pi_coding_agent::core::multi_agent_store::{
    AgentSnapshot, AgentSubscription, MultiAgentStore, MultiAgentStoreOptions,
};
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsValue, from_value};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

type Listener = Arc<dyn Fn() -> Result<(), CallbackError> + Send + Sync>;
fn subscribe(store: &MultiAgentStore, kind: &str, listener: Listener) -> AgentSubscription {
    match kind {
        "update" => store.subscribe_agent_updates(move |_, _| listener()),
        "transition" => store.subscribe_agent_transitions(move |_, _| listener()),
        "notification" => store.subscribe_lifecycle_notifications(move |_| listener()),
        _ => panic!("unknown listener kind"),
    }
}
fn snapshot(lifecycle: &str, revision: i64) -> Value {
    json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker","lifecycle":lifecycle,"revision":revision,
        "createdAt":"2026-10-25T00:00:00.000Z","updatedAt":"2026-10-25T00:00:00.000Z","cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}})
}

#[test]
fn membership_removals_and_additions_take_effect_in_the_current_notification() {
    let mut source = oracle::TsOracle::spawn(
        "storage-listener-oracle.mts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    for kind in ["update", "transition", "notification"] {
        for mutation in ["remove", "add"] {
            let running = snapshot("running", 1);
            let updated = snapshot(
                match kind {
                    "update" => "running",
                    "transition" => "waiting_for_input",
                    "notification" => "completed",
                    _ => unreachable!(),
                },
                2,
            );
            let expected =
                source.request(&json!({"id":1,"kind":kind,"mutation":mutation,"running":running,"updated":updated}));
            let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions {
                now: Some(Arc::new(|| Ok("2026-10-25T00:00:00.000Z".into()))),
            }));
            let running: AgentSnapshot = from_value(&JsValue::from(running)).unwrap();
            let updated: AgentSnapshot = from_value(&JsValue::from(updated)).unwrap();
            store.publish_lifecycle_coordinator_snapshot(&running).unwrap();
            let observed = Arc::new(Mutex::new(Vec::<String>::new()));
            let second = Arc::new(Mutex::new(None::<AgentSubscription>));
            let target = Arc::clone(&second);
            let events = Arc::clone(&observed);
            let listener_store = Arc::clone(&store);
            let _first = subscribe(
                &store,
                kind,
                Arc::new(move || {
                    events.lock().unwrap().push("first".into());
                    if mutation == "remove" {
                        target.lock().unwrap().take();
                    } else {
                        let events = Arc::clone(&events);
                        let subscription = subscribe(
                            &listener_store,
                            kind,
                            Arc::new(move || {
                                events.lock().unwrap().push("late".into());
                                Ok(())
                            }),
                        );
                        *target.lock().unwrap() = Some(subscription);
                    }
                    Ok(())
                }),
            );
            if mutation == "remove" {
                let events = Arc::clone(&observed);
                *second.lock().unwrap() = Some(subscribe(
                    &store,
                    kind,
                    Arc::new(move || {
                        events.lock().unwrap().push("second".into());
                        Ok(())
                    }),
                ));
            }
            if kind == "notification" {
                store.publish_terminal_outbox_snapshot(&updated).unwrap();
            } else {
                store.publish_lifecycle_coordinator_snapshot(&updated).unwrap();
            }
            assert_eq!(
                json!(*observed.lock().unwrap()),
                expected["observed"],
                "{kind} {mutation}"
            );
        }
    }
}
