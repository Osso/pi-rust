use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pi_agent_core::harness::events::{HarnessEvent, HarnessEventBus};
use serde_json::{Map, Value, json};

use super::support::{TsOracle, expect_ok, oracle_request};

fn run_start(lane: &str, run_id: &str) -> Value {
    json!({"type": "run_start", "lane": lane, "runId": run_id})
}

fn run_end(lane: &str, run_id: &str, outcome: &str, leaf_id: &str) -> Value {
    json!({"type": "run_end", "lane": lane, "runId": run_id, "outcome": outcome, "leafId": leaf_id})
}

fn scripts() -> Vec<Vec<Value>> {
    vec![
        vec![
            json!({"op": "on", "tag": "direct", "type": "run_start"}),
            json!({"op": "watch", "tag": "watch", "snapshot": {"leafId": null}}),
            json!({"op": "start", "tag": "watch"}),
            json!({"op": "emit", "event": run_start("main", "run-1")}),
            json!({"op": "emit", "event": run_end("main", "run-1", "completed", "entry-1")}),
            json!({"op": "off", "tag": "direct"}),
            json!({"op": "emit", "event": run_start("main", "run-1")}),
        ],
        vec![
            json!({
                "op": "watch",
                "tag": "buffered",
                "snapshot": ["snapshot", 1],
                "emitDuringSnapshot": run_start("snapshot-lane", "snapshot-run")
            }),
            json!({"op": "start", "tag": "buffered"}),
            json!({"op": "emit", "event": run_end("main", "run-2", "failed", "leaf")}),
            json!({"op": "unsubscribe", "tag": "buffered"}),
            json!({"op": "emit", "event": run_start("main", "run-3")}),
        ],
        vec![
            json!({"op": "on", "tag": "reentrant", "type": "run_start", "thenEmit": run_end("main", "run-1", "aborted", "leaf-2")}),
            json!({"op": "watch", "tag": "watcher", "snapshot": null}),
            json!({"op": "start", "tag": "watcher"}),
            json!({"op": "emit", "event": run_start("main", "run-1")}),
        ],
        vec![
            json!({"op": "on", "tag": "first", "type": "run_end"}),
            json!({"op": "on", "tag": "second", "type": "run_end"}),
            json!({"op": "emit", "event": run_end("a", "r", "completed", "l")}),
            json!({"op": "off", "tag": "first"}),
            json!({"op": "emit", "event": run_end("b", "r", "failed", "l")}),
            json!({"op": "off", "tag": "second"}),
            json!({"op": "emit", "event": run_end("c", "r", "aborted", "l")}),
        ],
        vec![
            json!({"op": "watch", "tag": "flushed", "snapshot": "snapshot-value"}),
            json!({"op": "emit", "event": run_start("main", "before-start")}),
            json!({"op": "start", "tag": "flushed"}),
            json!({"op": "start", "tag": "flushed"}),
            json!({"op": "emit", "event": run_start("main", "after-start")}),
        ],
        vec![],
    ]
}

fn parse_event(value: &Value) -> HarnessEvent {
    serde_json::from_value(value.clone()).expect("fixture event must deserialize")
}

fn event_json(event: &HarnessEvent) -> Value {
    serde_json::to_value(event).expect("event must serialize")
}

fn run_script(script: &[Value]) -> Value {
    let bus = HarnessEventBus::new();
    let direct: Arc<Mutex<HashMap<String, Vec<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
    let watched: Arc<Mutex<HashMap<String, Vec<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
    let snapshots: Arc<Mutex<HashMap<String, Value>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut unsubscribes: HashMap<String, Box<dyn FnOnce()>> = HashMap::new();
    let mut watches: HashMap<String, pi_agent_core::harness::events::WatchHandle<Value>> = HashMap::new();

    for operation in script {
        match operation["op"].as_str().expect("script op") {
            "on" => {
                let tag = operation["tag"].as_str().unwrap().to_owned();
                let event_type = match operation["type"].as_str().unwrap() {
                    "run_start" => "run_start",
                    "run_end" => "run_end",
                    other => panic!("unknown event type {other}"),
                };
                direct.lock().unwrap().insert(tag.clone(), Vec::new());
                let sink = Arc::clone(&direct);
                let tag_for_listener = tag.clone();
                let bus_for_listener = bus.clone();
                let then_emit = operation.get("thenEmit").cloned();
                let subscription = bus.on(
                    event_type,
                    Arc::new(move |event: &HarnessEvent| {
                        sink.lock()
                            .unwrap()
                            .get_mut(&tag_for_listener)
                            .unwrap()
                            .push(event_json(event));
                        if let Some(next) = &then_emit {
                            bus_for_listener.emit(parse_event(next));
                        }
                    }),
                );
                unsubscribes.insert(tag, Box::new(move || subscription.unsubscribe()));
            }
            "off" => {
                if let Some(unsubscribe) = unsubscribes.remove(operation["tag"].as_str().unwrap()) {
                    unsubscribe();
                }
            }
            "watch" => {
                let tag = operation["tag"].as_str().unwrap().to_owned();
                let snapshot = operation.get("snapshot").cloned().unwrap_or(Value::Null);
                let emit_during_snapshot = operation.get("emitDuringSnapshot").cloned();
                watched.lock().unwrap().insert(tag.clone(), Vec::new());
                let emitter = bus.clone();
                let captured = snapshot.clone();
                let watch = bus.watch(move || {
                    if let Some(event) = &emit_during_snapshot {
                        emitter.emit(parse_event(event));
                    }
                    captured
                });
                snapshots.lock().unwrap().insert(tag.clone(), watch.snapshot().clone());
                watches.insert(tag, watch);
            }
            "start" => {
                let tag = operation["tag"].as_str().unwrap().to_owned();
                let sink = Arc::clone(&watched);
                let tag_for_listener = tag.clone();
                if let Some(watch) = watches.get_mut(&tag) {
                    watch.start(Arc::new(move |event: &HarnessEvent| {
                        sink.lock()
                            .unwrap()
                            .get_mut(&tag_for_listener)
                            .unwrap()
                            .push(event_json(event));
                    }));
                }
            }
            "unsubscribe" => {
                if let Some(watch) = watches.get_mut(operation["tag"].as_str().unwrap()) {
                    watch.unsubscribe();
                }
            }
            "emit" => bus.emit(parse_event(&operation["event"])),
            other => panic!("unknown script op {other}"),
        }
    }

    let to_map = |values: Arc<Mutex<HashMap<String, Vec<Value>>>>| -> Value {
        let values = values.lock().unwrap();
        Value::Object(
            values
                .iter()
                .map(|(key, events)| (key.clone(), Value::Array(events.clone())))
                .collect::<Map<String, Value>>(),
        )
    };
    json!({
        "direct": to_map(direct),
        "watched": to_map(watched),
        "snapshots": Value::Object(snapshots.lock().unwrap().clone().into_iter().collect()),
    })
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    for (index, script) in scripts().into_iter().enumerate() {
        let id = 14_000 + index as u64;
        let request = oracle_request(id, "event_bus", json!({"script": script}));
        let response = oracle.request(&request);
        let expected = expect_ok(&response);
        assert_eq!(run_script(&script), *expected, "events fixture {id}: {script:?}");
    }
}
