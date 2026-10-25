use crate::shared_oracle as oracle;

use pi_coding_agent::core::tool_detach_registry::{DEFAULT_AUTO_DETACH_AFTER_MS, ToolDetachHandle, ToolDetachRegistry};
use pi_js_compat::{js_json as json, js_json::JsValue};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

struct Handle {
    name: String,
    result: bool,
    calls: Arc<Mutex<Vec<String>>>,
}
impl ToolDetachHandle for Handle {
    fn detach(&self) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
        self.calls.lock().unwrap().push(self.name.clone());
        Ok(self.result)
    }
}
fn sync_cases() -> JsValue {
    json!([
        {"handles":[],"steps":[{"op":"has"},{"op":"running"},{"op":"all"}]},
        {"handles":[{"name":"first","result":true},{"name":"second","result":false}],"steps":[{"op":"register","name":"first"},{"op":"register","name":"second"},{"op":"running"},{"op":"has"},{"op":"all"}]},
        {"handles":[{"name":"first","result":true},{"name":"second","result":true}],"steps":[{"op":"register","name":"first"},{"op":"register","name":"second"},{"op":"running"},{"op":"all"},{"op":"unregister","index":0},{"op":"running"},{"op":"unregister","index":1},{"op":"has"}]},
        {"handles":[{"name":"first","result":false},{"name":"second","result":false}],"steps":[{"op":"register","name":"first"},{"op":"register","name":"second"},{"op":"running"},{"op":"all"}]},
        {"handles":[{"name":"same","result":true}],"steps":[{"op":"register","name":"same"},{"op":"register","name":"same"},{"op":"all"},{"op":"unregister","index":0},{"op":"has"},{"op":"running"},{"op":"unregister","index":1}]}
    ])
}
fn native_history(input: &JsValue) -> JsValue {
    let registry = Arc::new(ToolDetachRegistry::new());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let handles = input["handles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            let name = input["name"].as_string().unwrap().to_utf8().unwrap();
            (
                name.clone(),
                Arc::new(Handle {
                    name,
                    result: input["result"].as_bool().unwrap(),
                    calls: calls.clone(),
                }),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut subscriptions = Vec::new();
    let mut states = Vec::new();
    for step in input["steps"].as_array().unwrap() {
        let op = step["op"].as_string().unwrap().to_utf8().unwrap();
        let mut result = JsValue::Null;
        match op.as_str() {
            "register" => {
                subscriptions.push(Some(
                    registry.register(
                        handles
                            .get(&step["name"].as_string().unwrap().to_utf8().unwrap())
                            .unwrap()
                            .clone(),
                    ),
                ));
            }
            "unregister" => {
                subscriptions[step["index"].as_u64().unwrap() as usize]
                    .take()
                    .unwrap()
                    .unregister();
            }
            "running" => {
                result = json!(registry.detach_running().expect("fixture detach callback"));
            }
            "all" => registry.detach_all().expect("fixture detach callback"),
            "has" => {}
            _ => panic!("unknown fixture operation"),
        }
        states.push(
            json!({"op":op,"result":result,"calls":calls.lock().unwrap().clone(),"hasRunning":registry.has_running()}),
        );
    }
    json!(states)
}
#[test]
fn full_registry_histories_match_actual_frozen_set_and_detach_behavior() {
    let cases = sync_cases();
    let mut oracle = oracle::TsOracle::spawn(
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
    );
    let expected = oracle.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let actual = json!({"defaultDelay":DEFAULT_AUTO_DETACH_AFTER_MS,"histories":cases.as_array().unwrap().iter().map(native_history).collect::<Vec<_>>()});
    assert_eq!(actual["defaultDelay"], expected["output"]["defaultDelay"]);
    for (index, history) in actual["histories"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            history, &expected["output"]["histories"][index],
            "registry history {index}"
        );
    }
}

struct TimerHandle(std::sync::mpsc::Sender<()>);
impl ToolDetachHandle for TimerHandle {
    fn detach(&self) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
        self.0.send(()).expect("live timer receiver");
        Ok(true)
    }
}
#[test]
fn retained_subscription_keeps_the_registry_timer_alive_after_the_caller_drops_it() {
    let mut oracle = oracle::TsOracle::spawn(
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
    );
    let expected = oracle.request(&json!({"id":2,"operation":"retainedSubscriptionTimer"}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"],
        json!({"called":true}),
        "source ref'ed guard must remain alive until the unref'ed detach timer fires"
    );
    let (send, receive) = std::sync::mpsc::channel();
    let subscription = {
        let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(20.0));
        registry.register(Arc::new(TimerHandle(send)))
    };
    let called = receive.recv_timeout(std::time::Duration::from_secs(1)).is_ok();
    drop(subscription);
    assert_eq!(json!({"called":called}), expected["output"]);
}

#[test]
fn overflow_delay_uses_the_frozen_one_millisecond_timer_range() {
    let mut oracle = oracle::TsOracle::spawn(
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
    );
    let expected = oracle.request(&json!({"id":4,"operation":"timerDelay","delay":2147483648_u64}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(expected["output"], json!({"called":true}));
    let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(2147483648.0));
    let (send, receive) = std::sync::mpsc::channel();
    let subscription = registry.register(Arc::new(TimerHandle(send)));
    let called = receive.recv_timeout(std::time::Duration::from_millis(200)).is_ok();
    drop(subscription);
    assert_eq!(json!({"called":called}), expected["output"]);
}
