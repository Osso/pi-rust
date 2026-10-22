use std::sync::{Arc, Mutex};

use pi_coding_agent::core::event_bus::{EventBus, EventBusController, create_event_bus};
use pi_js_compat::js_json::{JsString, JsValue};

type Handler = Box<dyn Fn(&JsValue) -> Result<(), JsString> + Send + Sync>;

fn recorder() -> (Arc<Mutex<Vec<String>>>, impl Fn(&'static str) -> Handler) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&calls);
    let handler = move |name: &'static str| {
        let calls = Arc::clone(&captured);
        Box::new(move |_data: &JsValue| {
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
    let _subscription = bus.on_handler(&JsString::from("reload:test"), move |data| {
        sink.lock().unwrap().push(data.clone());
        Ok(())
    });

    assert!(
        bus.emit(
            &JsString::from("reload:test"),
            JsValue::from(serde_json::json!({"count": 2}))
        )
        .is_empty()
    );
    assert!(
        bus.emit(&JsString::from("other"), JsValue::from(serde_json::json!(1)))
            .is_empty()
    );
    assert!(bus.emit(&JsString::from("reload:test"), JsValue::Null).is_empty());

    assert_eq!(
        *received.lock().unwrap(),
        vec![JsValue::from(serde_json::json!({"count": 2})), JsValue::Null]
    );
}

#[test]
fn unsubscribe_removes_only_that_listener() {
    let bus = create_event_bus();
    let (calls, handler) = recorder();
    let first = bus.on_handler(&JsString::from("reload:test"), handler("first"));
    let _second = bus.on_handler(&JsString::from("reload:test"), handler("second"));

    bus.emit(&JsString::from("reload:test"), JsValue::Null);
    first.unsubscribe();
    bus.emit(&JsString::from("reload:test"), JsValue::Null);

    assert_eq!(*calls.lock().unwrap(), vec!["first", "second", "second"]);
}

#[test]
fn clear_drops_every_listener() {
    let bus = create_event_bus();
    let (calls, handler) = recorder();
    let _first = bus.on_handler(&JsString::from("a"), handler("a"));
    let _second = bus.on_handler(&JsString::from("b"), handler("b"));

    bus.clear();
    bus.emit(&JsString::from("a"), JsValue::Null);
    bus.emit(&JsString::from("b"), JsValue::Null);

    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn one_failing_listener_does_not_stop_the_others() {
    let bus = create_event_bus();
    let (calls, handler) = recorder();
    let _before = bus.on_handler(&JsString::from("reload:test"), handler("before"));
    let _failing = bus.on_handler(&JsString::from("reload:test"), |_| {
        Err(JsString::from("handler exploded"))
    });
    let _after = bus.on_handler(&JsString::from("reload:test"), handler("after"));

    let errors = bus.emit(&JsString::from("reload:test"), JsValue::Null);

    assert_eq!(*calls.lock().unwrap(), vec!["before", "after"]);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].channel, "reload:test");
    assert_eq!(errors[0].message, "handler exploded");
}

#[test]
fn listeners_can_unsubscribe_from_inside_a_delivery() {
    let bus = Arc::new(create_event_bus());
    let (calls, handler) = recorder();
    let other = Arc::new(Mutex::new(Some(
        bus.on_handler(&JsString::from("reload:test"), handler("other")),
    )));
    let weak_bus = Arc::downgrade(&bus);
    let subscription = bus.on_handler(&JsString::from("reload:test"), move |_| {
        let unsubscribed = other.lock().unwrap().take().is_some();
        if let Some(bus) = weak_bus.upgrade()
            && !unsubscribed
        {
            let _ = bus.emit(&JsString::from("reload:test"), JsValue::Null);
        }
        Ok(())
    });

    bus.emit(&JsString::from("reload:test"), JsValue::Null);
    subscription.unsubscribe();
    bus.emit(&JsString::from("reload:test"), JsValue::Null);

    assert_eq!(*calls.lock().unwrap(), vec!["other", "other"]);
}

#[test]
fn event_bus_controller_is_an_event_bus() {
    fn takes_bus(_: &dyn EventBus) {}
    let bus: EventBusController = create_event_bus();
    takes_bus(&bus);
}

/// `emitter.emit(channel, data)` passes JS strings through unchanged, including lone surrogates.
#[test]
fn channel_and_payload_keep_raw_utf16_units() {
    let bus = create_event_bus();
    let channel = JsString::from("raw:") + JsString::from_utf16(vec![0xd800]);
    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&received);
    let _subscription = bus.on_handler(&channel, move |data: &JsValue| {
        sink.lock().unwrap().push(data.clone());
        Err(JsString::from_utf16(vec![0x65, 0xdfff]))
    });
    let payload = JsValue::object([("text", JsValue::String(JsString::from_utf16(vec![0xdc00, 0x61])))]);

    assert!(
        bus.emit(
            &(JsString::from("raw:") + JsString::from_utf16(vec![0xfffd])),
            payload.clone()
        )
        .is_empty()
    );
    let errors = bus.emit(&channel, payload.clone());

    assert_eq!(*received.lock().unwrap(), vec![payload]);
    assert_eq!(errors[0].channel, channel);
    assert_eq!(errors[0].message.units(), [0x65, 0xdfff]);
}
