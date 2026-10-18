//! Behavioral port of `packages/agent/test/harness/events.test.ts`.

use std::sync::Arc;
use std::sync::Mutex;

use pi_agent_core::harness::events::{HarnessEvent, HarnessEventBus, RunEndOutcome, RunStartEvent};

fn run_start() -> HarnessEvent {
    HarnessEvent::RunStart(RunStartEvent {
        lane: "main".to_owned(),
        run_id: "run-1".to_owned(),
    })
}

fn run_end() -> HarnessEvent {
    HarnessEvent::RunEnd {
        lane: "main".to_owned(),
        run_id: "run-1".to_owned(),
        outcome: RunEndOutcome::Completed,
        leaf_id: "entry-1".to_owned(),
    }
}

type Recorder = (Arc<Mutex<Vec<HarnessEvent>>>, Arc<dyn Fn(&HarnessEvent) + Send + Sync>);

fn recorder() -> Recorder {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let listener: Arc<dyn Fn(&HarnessEvent) + Send + Sync> =
        Arc::new(move |event: &HarnessEvent| sink.lock().unwrap().push(event.clone()));
    (events, listener)
}

#[test]
fn delivers_matching_events_to_direct_listeners_and_watchers() {
    let bus = HarnessEventBus::new();
    let (direct, direct_listener) = recorder();
    let (watched, watch_listener) = recorder();
    let subscription = bus.on("run_start", direct_listener);
    let mut watch = bus.watch(|| None::<String>);
    watch.start(watch_listener);

    bus.emit(run_start());
    bus.emit(run_end());
    subscription.unsubscribe();
    bus.emit(run_start());

    assert_eq!(*direct.lock().unwrap(), vec![run_start()]);
    assert_eq!(*watched.lock().unwrap(), vec![run_start(), run_end(), run_start()]);
}

#[test]
fn captures_a_snapshot_without_an_event_gap_then_flushes_and_delivers_live_events() {
    let bus = HarnessEventBus::new();
    let emitter = bus.clone();
    let watch = bus.watch(move || {
        emitter.emit(run_start());
        "snapshot-leaf"
    });
    assert_eq!(watch.snapshot(), &"snapshot-leaf");

    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&received);
    let mut watch = watch;
    watch.start(Arc::new(move |event: &HarnessEvent| {
        sink.lock().unwrap().push(event.clone())
    }));
    assert_eq!(*received.lock().unwrap(), vec![run_start()]);

    bus.emit(run_end());
    assert_eq!(*received.lock().unwrap(), vec![run_start(), run_end()]);

    watch.unsubscribe();
    bus.emit(run_start());
    assert_eq!(*received.lock().unwrap(), vec![run_start(), run_end()]);
}

#[test]
fn reentrant_emissions_during_flush_preserve_order() {
    let bus = HarnessEventBus::new();
    let received = Arc::new(Mutex::new(Vec::new()));
    let mut watch = bus.watch(|| ());
    let sink = Arc::clone(&received);
    let bus_for_listener = bus.clone();
    watch.start(Arc::new(move |event: &HarnessEvent| {
        sink.lock().unwrap().push(event.clone());
        if matches!(event, HarnessEvent::RunStart(_)) {
            bus_for_listener.emit(run_end());
        }
    }));

    bus.emit(run_start());
    assert_eq!(*received.lock().unwrap(), vec![run_start(), run_end()]);
}

#[test]
fn event_type_literals_and_payload_json_match_the_source() {
    assert_eq!(run_start().event_type(), "run_start");
    assert_eq!(run_end().event_type(), "run_end");
    assert_eq!(
        serde_json::to_value(run_start()).unwrap(),
        serde_json::json!({"type": "run_start", "lane": "main", "runId": "run-1"})
    );
    assert_eq!(
        serde_json::to_value(run_end()).unwrap(),
        serde_json::json!({
            "type": "run_end",
            "lane": "main",
            "runId": "run-1",
            "outcome": "completed",
            "leafId": "entry-1"
        })
    );
}
