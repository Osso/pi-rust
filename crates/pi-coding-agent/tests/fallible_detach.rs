use crate::shared_oracle as oracle;
use pi_coding_agent::core::tool_detach_registry::{ToolDetachHandle, ToolDetachRegistry};
use pi_js_compat::{
    abort::AbortReason,
    callback_error::CallbackError,
    js_json as json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::sync::{Arc, Mutex};
struct ThrowingHandle {
    reason: CallbackError,
    calls: Arc<Mutex<Vec<String>>>,
}
impl ToolDetachHandle for ThrowingHandle {
    fn detach(&self) -> Result<bool, CallbackError> {
        self.calls.lock().unwrap().push("throwing".into());
        Err(self.reason.clone())
    }
}
struct ContinuingHandle {
    name: &'static str,
    calls: Arc<Mutex<Vec<String>>>,
}
impl ToolDetachHandle for ContinuingHandle {
    fn detach(&self) -> Result<bool, CallbackError> {
        self.calls.lock().unwrap().push(self.name.into());
        Ok(false)
    }
}
fn snapshot(error: CallbackError) -> JsValue {
    let reason = error.thrown_value();
    let kind = if matches!(reason, AbortReason::Error(_) | AbortReason::DomException(_)) {
        "error"
    } else {
        "value"
    };
    json!({"kind":kind,"value":reason.value()})
}
fn fixture_error(input: &JsValue) -> CallbackError {
    let reason = if input["kind"] == json!("error") {
        AbortReason::Error(
            NativeNodeError::from_constructor(NativeErrorClass::TypeError, input["value"].as_string().unwrap().clone())
                .with_property("code", json!("E_DETACH")),
        )
    } else {
        AbortReason::Value(input["value"].clone())
    };
    CallbackError::from_rejection(reason)
}
#[test]
fn registry_synchronous_methods_preserve_original_callback_rejections() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let inputs = json!([{"kind":"error","value":raw},{"kind":"value","value":raw},{"kind":"value","value":null},{"kind":"value","value":{"message":raw}}]);
    let mut oracle = oracle::TsOracle::spawn(
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
    );
    for input in inputs.as_array().unwrap() {
        let expected=oracle.request(&json!({"id":1,"cases":[{"handles":[{"name":"first","result":false},{"name":"throwing","result":false,"throw":input},{"name":"last","result":false}],"steps":[{"op":"register","name":"first"},{"op":"register","name":"throwing"},{"op":"register","name":"last"},{"op":"running"},{"op":"all"}]}]}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let reason = fixture_error(input);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let registry = Arc::new(ToolDetachRegistry::new());
        let _first = registry.register(Arc::new(ContinuingHandle {
            name: "first",
            calls: calls.clone(),
        }));
        let _subscription = registry.register(Arc::new(ThrowingHandle {
            reason,
            calls: calls.clone(),
        }));
        let _last = registry.register(Arc::new(ContinuingHandle {
            name: "last",
            calls: calls.clone(),
        }));
        let running = snapshot(registry.detach_running().unwrap_err());
        assert_eq!(running, expected["output"]["histories"][0][3]["error"]);
        let all = snapshot(registry.detach_all().unwrap_err());
        assert_eq!(all, expected["output"]["histories"][0][4]["error"]);
        assert_eq!(
            json!(calls.lock().unwrap().clone()),
            expected["output"]["histories"][0][4]["calls"]
        );
        assert!(registry.has_running());
    }
}

fn marker(bytes: &[u8], prefix: &str) -> JsValue {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let data = text
        .lines()
        .find_map(|line| line.split_once(prefix).map(|(_, data)| data))
        .expect("original uncaught rejection marker");
    pi_js_compat::js_json::parse_lossless(data).unwrap()
}
#[test]
fn automatic_detach_rejection_matches_the_actual_frozen_uncaught_process_boundary() {
    use std::io::Write;
    let raw = JsString::from("timer-") + JsString::from_utf16(vec![0xd800]);
    for input in [json!({"kind":"error","value":raw}), json!({"kind":"value","value":raw})] {
        let lock = oracle::read_source_lock();
        oracle::verify_source_head(&lock);
        let mut source = oracle::spawn_oracle_process(
            &lock,
            "tool-detach-registry-oracle.ts",
            "packages/coding-agent/src/core/tool-detach-registry.ts",
            &[],
            std::process::Stdio::piped(),
        );
        let mut stdin = source.stdin.take().unwrap();
        writeln!(
            stdin,
            "{}",
            pi_js_compat::js_json::stringify_lossless(&json!({"id":1,"operation":"timerThrow","reason":input}))
        )
        .unwrap();
        drop(stdin);
        let expected = source.wait_with_output().unwrap();
        assert_eq!(
            expected.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let native = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "automatic_detach_throw_probe", "--ignored", "--nocapture"])
            .env("PI_DETACH_THROW", pi_js_compat::js_json::stringify_lossless(&input))
            .output()
            .unwrap();
        assert_eq!(
            native.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(
            marker(&native.stdout, "NATIVE_TIMER_THROW="),
            marker(&expected.stdout, "FROZEN_TIMER_THROW=")
        );
        if input["kind"] == json!("error") {
            assert!(String::from_utf8(native.stderr).unwrap().contains("TypeError: timer-�"));
        }
    }
}
#[test]
#[ignore = "explicit private child probe for the timer uncaught boundary"]
fn automatic_detach_throw_probe() {
    use std::io::Write;
    let input =
        pi_js_compat::js_json::parse_lossless(std::env::var("PI_DETACH_THROW").expect("private timer probe input"))
            .unwrap();
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(|payload| {
        let error = payload
            .downcast_ref::<CallbackError>()
            .expect("original timer callback error");
        let mut output = std::io::stdout().lock();
        writeln!(
            output,
            "NATIVE_TIMER_THROW={}",
            pi_js_compat::js_json::stringify_lossless(&snapshot(error.clone()))
        )
        .unwrap();
        output.flush().unwrap();
    })));
    let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(1.0));
    let _subscription = registry.register(Arc::new(ThrowingHandle {
        reason: fixture_error(&input),
        calls: Arc::new(Mutex::new(Vec::new())),
    }));
    std::thread::sleep(std::time::Duration::from_secs(5));
    panic!("automatic callback did not throw");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
