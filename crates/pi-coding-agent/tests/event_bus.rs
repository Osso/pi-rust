use std::sync::{Arc, Mutex};

use pi_coding_agent::core::event_bus::{EventBus, EventBusController, create_event_bus};

type Handler = Box<dyn Fn(&serde_json::Value) -> Result<(), String> + Send + Sync>;

fn recorder() -> (Arc<Mutex<Vec<String>>>, impl Fn(&'static str) -> Handler) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&calls);
    let handler = move |name: &'static str| {
        let calls = Arc::clone(&captured);
        Box::new(move |_data: &serde_json::Value| {
            calls.lock().unwrap().push(name.to_owned());
            Ok(())
        }) as Handler
    };
    (calls, handler)
}

#[test]
fn delivers_emitted_data_to_channel_listeners() {
    let bus = create_event_bus();
    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&received);
    let _subscription = bus.on_handler("reload:test", move |data| {
        sink.lock().unwrap().push(data.clone());
        Ok(())
    });

    assert!(bus.emit("reload:test", serde_json::json!({"count": 2})).is_empty());
    assert!(bus.emit("other", serde_json::json!(1)).is_empty());
    assert!(bus.emit("reload:test", serde_json::Value::Null).is_empty());

    assert_eq!(
        *received.lock().unwrap(),
        vec![serde_json::json!({"count": 2}), serde_json::Value::Null]
    );
}

#[test]
fn unsubscribe_removes_only_that_listener() {
    let bus = create_event_bus();
    let (calls, handler) = recorder();
    let first = bus.on_handler("reload:test", handler("first"));
    let _second = bus.on_handler("reload:test", handler("second"));

    bus.emit("reload:test", serde_json::Value::Null);
    first.unsubscribe();
    bus.emit("reload:test", serde_json::Value::Null);

    assert_eq!(*calls.lock().unwrap(), vec!["first", "second", "second"]);
}

#[test]
fn clear_drops_every_listener() {
    let bus = create_event_bus();
    let (calls, handler) = recorder();
    let _first = bus.on_handler("a", handler("a"));
    let _second = bus.on_handler("b", handler("b"));

    bus.clear();
    bus.emit("a", serde_json::Value::Null);
    bus.emit("b", serde_json::Value::Null);

    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn one_failing_listener_does_not_stop_the_others() {
    let bus = create_event_bus();
    let (calls, handler) = recorder();
    let _before = bus.on_handler("reload:test", handler("before"));
    let _failing = bus.on_handler("reload:test", |_| Err("handler exploded".to_owned()));
    let _after = bus.on_handler("reload:test", handler("after"));

    let errors = bus.emit("reload:test", serde_json::Value::Null);

    assert_eq!(*calls.lock().unwrap(), vec!["before", "after"]);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].channel, "reload:test");
    assert_eq!(errors[0].message, "handler exploded");
}

#[test]
fn listeners_can_unsubscribe_from_inside_a_delivery() {
    let bus = Arc::new(create_event_bus());
    let (calls, handler) = recorder();
    let other = Arc::new(Mutex::new(Some(bus.on_handler("reload:test", handler("other")))));
    let weak_bus = Arc::downgrade(&bus);
    let subscription = bus.on_handler("reload:test", move |_| {
        let unsubscribed = other.lock().unwrap().take().is_some();
        if let Some(bus) = weak_bus.upgrade()
            && !unsubscribed
        {
            let _ = bus.emit("reload:test", serde_json::Value::Null);
        }
        Ok(())
    });

    bus.emit("reload:test", serde_json::Value::Null);
    subscription.unsubscribe();
    bus.emit("reload:test", serde_json::Value::Null);

    assert_eq!(*calls.lock().unwrap(), vec!["other", "other"]);
}

#[test]
fn event_bus_controller_is_an_event_bus() {
    fn takes_bus(_: &dyn EventBus) {}
    let bus: EventBusController = create_event_bus();
    takes_bus(&bus);
}
