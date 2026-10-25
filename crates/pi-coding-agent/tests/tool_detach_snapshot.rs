use crate::shared_oracle as oracle;
use pi_coding_agent::core::tool_detach_registry::{DetachSubscription, ToolDetachHandle, ToolDetachRegistry};
use pi_js_compat::{callback_error::CallbackError, js_json as json};
use std::sync::{Arc, Mutex};
struct ActionHandle(Box<dyn Fn() -> Result<bool, CallbackError> + Send + Sync>);
impl ToolDetachHandle for ActionHandle {
    fn detach(&self) -> Result<bool, CallbackError> {
        (self.0)()
    }
}
#[test]
fn both_detach_methods_keep_their_snapshot_when_callbacks_unregister_and_register_handles() {
    let mut source = oracle::TsOracle::spawn(
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
    );
    let expected = source.request(&json!({"id":1,"operation":"mutatingSnapshots"}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let registry = Arc::new(ToolDetachRegistry::new());
    let subscriptions = Arc::new(Mutex::new(Vec::<DetachSubscription>::new()));
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let third_calls = calls.clone();
    let third_subscriptions = Arc::downgrade(&subscriptions);
    let third = Arc::new(ActionHandle(Box::new(move || {
        third_calls.lock().unwrap().push("third".into());
        third_subscriptions
            .upgrade()
            .expect("fixture subscriptions")
            .lock()
            .unwrap()[0]
            .unregister();
        Ok(false)
    })));
    let first_calls = calls.clone();
    let first_subscriptions = Arc::downgrade(&subscriptions);
    let first_registry = Arc::downgrade(&registry);
    let first = Arc::new(ActionHandle(Box::new(move || {
        first_calls.lock().unwrap().push("first".into());
        let subscriptions = first_subscriptions.upgrade().expect("fixture subscriptions");
        subscriptions.lock().unwrap()[1].unregister();
        let registered = first_registry
            .upgrade()
            .expect("fixture registry")
            .register(third.clone());
        subscriptions.lock().unwrap().push(registered);
        Ok(false)
    })));
    subscriptions.lock().unwrap().push(registry.register(first));
    let second_calls = calls.clone();
    subscriptions
        .lock()
        .unwrap()
        .push(registry.register(Arc::new(ActionHandle(Box::new(move || {
            second_calls.lock().unwrap().push("second".into());
            Ok(false)
        })))));
    registry.detach_all().unwrap();
    let all = json!({"calls":calls.lock().unwrap().clone(),"hasRunning":registry.has_running()});
    let result = registry.detach_running().unwrap();
    let actual = json!({"all":all,"running":{"result":result,"calls":calls.lock().unwrap().clone(),"hasRunning":registry.has_running()}});
    subscriptions.lock().unwrap().clear();
    assert_eq!(actual, expected["output"]);
    assert_eq!(actual["all"]["calls"], json!(["first", "second"]));
    assert_eq!(actual["running"]["calls"], json!(["first", "second", "third", "first"]));
}
