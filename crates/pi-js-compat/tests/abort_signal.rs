use crate::shared_oracle_0 as oracle;

use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal, AbortSubscription};
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::{js_json as json, number::js_number_to_string};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[test]
fn signal_equality_compares_shared_signal_not_current_reason() {
    let signal = AbortSignal::default();
    assert_eq!(signal, signal.clone());
    assert_ne!(signal, AbortSignal::default());
    assert_ne!(AbortSignal::abort(None), AbortSignal::abort(None));
}

#[test]
fn retained_listeners_match_node_owner_drop_removal_and_in_dispatch_removal() {
    let controller = AbortController::new();
    let signal = controller.signal();
    let events = Arc::new(Mutex::new(Vec::new()));
    {
        let events = events.clone();
        let _owner = signal.add_listener(move |_| events.lock().unwrap().push("retained"));
    }
    let seen = events.clone();
    let removed = signal.add_listener(move |_| seen.lock().unwrap().push("removed"));
    signal.remove_listener(&removed);
    let later = Arc::new(Mutex::new(None));
    let remove = later.clone();
    let source = signal.clone();
    let seen = events.clone();
    signal.add_listener(move |_| {
        seen.lock().unwrap().push("first");
        source.remove_listener(remove.lock().unwrap().as_ref().unwrap());
    });
    let seen = events.clone();
    *later.lock().unwrap() = Some(signal.add_listener(move |_| seen.lock().unwrap().push("during")));
    let seen = events.clone();
    signal.add_listener(move |_| seen.lock().unwrap().push("last"));
    controller.abort(Some(AbortReason::Value(json!("reason"))));
    let seen = events.clone();
    signal.add_listener(move |_| seen.lock().unwrap().push("late"));
    assert_eq!(
        json!(events.lock().unwrap().clone()),
        oracle().request(&json!({"mode":"retained-listeners"}))
    );
}

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn("abort-signal-oracle.ts", "packages/ai/src/utils/abort-signals.ts")
}
fn number(value: &str) -> f64 {
    match value {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        other => other.parse().unwrap(),
    }
}
fn decode(value: &JsValue) -> Option<AbortReason> {
    let kind = value["kind"].as_string()?.to_utf8().unwrap();
    if kind == "default" {
        return None;
    }
    if kind == "value" {
        return Some(AbortReason::Value(value["value"].clone()));
    }
    if kind == "number" {
        return Some(AbortReason::Value(JsValue::Number(number(
            &value["value"].as_string().unwrap().to_utf8().unwrap(),
        ))));
    }
    let name = value["name"].as_string().cloned().unwrap_or_else(|| "Error".into());
    let message = value["message"].as_string().cloned().unwrap_or_default();
    let mut error = NativeNodeError::new(name.clone(), message);
    if kind == "dom" {
        let code = if name == "AbortError" {
            20.0
        } else if name == "TimeoutError" {
            23.0
        } else {
            0.0
        };
        error = error.with_property("code", JsValue::Number(code));
    }
    if let Some(properties) = value["properties"].as_object() {
        for (name, value) in properties {
            error = error.with_property(name.clone(), value.clone());
        }
    }
    Some(if kind == "dom" {
        AbortReason::DomException(error)
    } else {
        AbortReason::Error(error)
    })
}
fn reason(reason: Option<AbortReason>) -> JsValue {
    match reason {
        None => json!({"kind":"undefined"}),
        Some(AbortReason::Value(JsValue::Number(value))) => {
            let text = if value == 0.0 && value.is_sign_negative() {
                "-0".into()
            } else {
                js_number_to_string(value)
            };
            json!({"kind":"number","value":text})
        }
        Some(AbortReason::Value(value)) => json!({"kind":"value","value":value}),
        Some(AbortReason::Error(error)) => json!({"kind":"error","value":error.value()}),
        Some(AbortReason::DomException(error)) => json!({"kind":"dom","value":error.value()}),
    }
}
fn snapshot(signal: &AbortSignal) -> JsValue {
    json!({"aborted":signal.aborted(),"reason":reason(signal.reason())})
}
fn controller_case(input: &JsValue) -> JsValue {
    let controller = AbortController::new();
    let signal = controller.signal();
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    let _listener = signal
        .subscribe(move |value| seen.lock().unwrap().push(reason(Some(value.clone()))))
        .unwrap();
    let before = snapshot(&signal);
    controller.abort(decode(&input["first"]));
    let after = snapshot(&signal);
    controller.abort(decode(&input["second"]));
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let late = signal.subscribe(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
    });
    assert!(late.is_err());
    json!({"before":before,"after":after,"afterSecond":snapshot(&signal),"events":events.lock().unwrap().clone(),"lateCalled":count.load(Ordering::SeqCst)})
}
fn any_case(input: &JsValue) -> JsValue {
    let controllers: Vec<_> = input["initial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let controller = AbortController::new();
            if !matches!(value, JsValue::Null) {
                controller.abort(decode(value));
            }
            controller
        })
        .collect();
    let sources: Vec<_> = controllers.iter().map(AbortController::signal).collect();
    let signal = AbortSignal::any(&sources);
    let mut snapshots = vec![snapshot(&signal)];
    for action in input["actions"].as_array().unwrap() {
        controllers[action["index"].as_f64().unwrap() as usize].abort(decode(&action["reason"]));
        snapshots.push(snapshot(&signal));
    }
    JsValue::Array(snapshots)
}
fn subscription_case() -> JsValue {
    let controller = AbortController::new();
    let signal = controller.signal();
    let events = Arc::new(Mutex::new(Vec::new()));
    let later: Arc<Mutex<Option<AbortSubscription>>> = Arc::new(Mutex::new(None));
    let remove = later.clone();
    let seen = events.clone();
    let _first = signal
        .subscribe(move |_| {
            seen.lock().unwrap().push("first");
            drop(remove.lock().unwrap().take());
        })
        .unwrap();
    let seen = events.clone();
    *later.lock().unwrap() = Some(signal.subscribe(move |_| seen.lock().unwrap().push("removed")).unwrap());
    let seen = events.clone();
    let _last = signal.subscribe(move |_| seen.lock().unwrap().push("last")).unwrap();
    controller.abort(Some(AbortReason::Value(json!("reason"))));
    controller.abort(Some(AbortReason::Value(json!("ignored"))));
    json!(events.lock().unwrap().clone())
}

#[tokio::test]
async fn reasons_first_abort_any_and_listener_order_match_node_and_frozen_combine() {
    let mut oracle = oracle();
    let raw = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let descriptors = vec![
        json!({"kind":"default"}),
        json!({"kind":"value","value":null}),
        json!({"kind":"value","value":false}),
        json!({"kind":"value","value":raw}),
        json!({"kind":"value","value":{"name":"AbortError","message":raw,"nested":[null,false,{"raw":raw}]}}),
        json!({"kind":"error","name":"Error","message":raw,"properties":{"code":"CUSTOM","cause":{"raw":raw}}}),
        json!({"kind":"error","name":"AbortError","message":"not a DOMException","properties":{"code":20}}),
        json!({"kind":"dom","name":"TimeoutError","message":raw}),
        json!({"kind":"number","value":"NaN"}),
        json!({"kind":"number","value":"-0"}),
        json!({"kind":"number","value":"Infinity"}),
        json!({"kind":"error","properties":{"name":raw,"message":raw,"stack":"not reproduced"}}),
        json!({"kind":"value","value":{"stack":raw}}),
    ];
    for first in descriptors {
        let input = json!({"mode":"controller","first":first,"second":{"kind":"value","value":"ignored"}});
        assert_eq!(controller_case(&input), oracle.request(&input));
        let static_input = json!({"mode":"static","first":input["first"]});
        assert_eq!(
            snapshot(&AbortSignal::abort(decode(&input["first"]))),
            oracle.request(&static_input)
        );
    }
    for engine in ["node", "frozen"] {
        for (initial, actions) in [
            (
                json!([null, null]),
                json!([{"index":1,"reason":{"kind":"value","value":raw}},{"index":0,"reason":{"kind":"default"}}]),
            ),
            (
                json!([{"kind":"value","value":null},{"kind":"error","message":"later"}]),
                json!([]),
            ),
            (
                json!([null,{"kind":"default"}]),
                json!([{"index":0,"reason":{"kind":"value","value":false}}]),
            ),
        ] {
            let input = json!({"mode":"any","engine":engine,"initial":initial,"actions":actions});
            assert_eq!(any_case(&input), oracle.request(&input));
        }
    }
    let empty = json!({"mode":"any","engine":"node","initial":[],"actions":[]});
    assert_eq!(any_case(&empty), oracle.request(&empty));
    assert_eq!(subscription_case(), oracle.request(&json!({"mode":"subscriptions"})));
}

#[tokio::test]
async fn timeout_validation_default_reason_and_integer_domain_match_node() {
    let mut oracle = oracle();
    for delay in [
        "0",
        "1",
        "-0",
        "NaN",
        "Infinity",
        "-Infinity",
        "-1",
        "1.5",
        "4294967296",
        "4294967297",
        "-4294967297",
        "9007199254740991",
        "1e20",
        "1e21",
        "1.7976931348623157e308",
        "2147483647",
        "2147483648",
        "4294967295",
    ] {
        let wait = matches!(delay, "0" | "1" | "-0" | "2147483648" | "4294967295");
        let input = json!({"mode":"timeout","delay":delay,"wait":wait});
        let expected = oracle.request(&input);
        let actual = match AbortSignal::timeout(number(delay)) {
            Err(error) => json!({"error":error.value()}),
            Ok(signal) => {
                let before = snapshot(&signal);
                if wait {
                    signal.wait().await;
                    json!({"before":before,"after":snapshot(&signal)})
                } else {
                    json!({"before":before})
                }
            }
        };
        assert_eq!(actual, expected, "delay={delay}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn awaiting_abort_cannot_lose_a_notification_and_composites_keep_sources_alive() {
    for iteration in 0..100 {
        let controller = AbortController::new();
        let signal = controller.signal();
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let ready = barrier.clone();
        let waiter = tokio::spawn(async move {
            ready.wait().await;
            signal.wait().await
        });
        barrier.wait().await;
        if iteration % 2 == 0 {
            tokio::task::yield_now().await;
        }
        let expected = AbortReason::Value(json!({"iteration":iteration}));
        controller.abort(Some(expected.clone()));
        assert_eq!(waiter.await.unwrap(), expected);
    }
    let signal = AbortSignal::any(&[AbortSignal::timeout(1.0).unwrap()]);
    let reason = tokio::time::timeout(std::time::Duration::from_secs(1), signal.wait())
        .await
        .unwrap();
    assert!(matches!(reason,AbortReason::DomException(error) if error.name=="TimeoutError"));
}

#[tokio::test]
async fn dropping_the_last_timeout_owner_releases_its_runtime_task() {
    let runtime = tokio::runtime::Handle::current();
    let baseline = runtime.metrics().num_alive_tasks();
    let signal = AbortSignal::timeout(60_000.0).unwrap();
    tokio::task::yield_now().await;
    assert!(runtime.metrics().num_alive_tasks() > baseline);
    drop(signal);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while runtime.metrics().num_alive_tasks() > baseline {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("unowned timeout must not retain an active runtime task");
}
