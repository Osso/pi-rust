#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::{
    api::lazy::lazy_api,
    request::{CallbackError, ProviderStreams, SimpleStreamOptions, StreamOptions},
    types::{Context, Model},
    utils::event_stream::AssistantMessageEventStream,
};
use pi_js_compat::{
    abort::AbortReason,
    js_json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn model_value() -> JsValue {
    js_json!({"id":"one","name":"One","api":"test-api","provider":"lazy-fixture","baseUrl":"https://fixture.invalid","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32})
}
fn rejection(kind: &str, value: &JsString) -> CallbackError {
    let reason = match kind {
        "error" => NativeNodeError::from_constructor(NativeErrorClass::TypeError, value.clone())
            .with_property("code", JsValue::String("E_LAZY".into()))
            .into(),
        "value" => AbortReason::Value(JsValue::String(value.clone())),
        "conversion" => AbortReason::Value(js_json!({"toString":null,"valueOf":null})),
        _ => panic!("fixture kind"),
    };
    CallbackError::from_rejection(reason)
}
struct ThrowingModule {
    error: CallbackError,
    calls: Arc<AtomicUsize>,
}
impl ProviderStreams for ThrowingModule {
    fn stream(&self, _: Model, _: Context, _: StreamOptions) -> Result<AssistantMessageEventStream, CallbackError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(self.error.clone())
    }
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        self.stream(model, context, options.stream)
    }
}
fn mask_timestamps(value: &mut JsValue, before: f64, after: f64) {
    match value {
        JsValue::Object(object) => {
            if let Some(timestamp) = object.get_mut("timestamp") {
                assert!((before..=after).contains(&timestamp.as_f64().unwrap()));
                *timestamp = js_json!(0);
            }
            for child in object.values_mut() {
                mask_timestamps(child, before, after);
            }
        }
        JsValue::Array(values) => {
            for child in values {
                mask_timestamps(child, before, after);
            }
        }
        _ => {}
    }
}
async fn native_snapshot(method: &str, kind: &str, value: &JsString) -> JsValue {
    let calls = Arc::new(AtomicUsize::new(0));
    let module: Arc<dyn ProviderStreams> = Arc::new(ThrowingModule {
        error: rejection(kind, value),
        calls: calls.clone(),
    });
    let api = lazy_api(Arc::new(move || {
        let module = module.clone();
        Box::pin(async move { Ok(module) })
    }));
    let model = js_json::from_value(&model_value()).unwrap();
    let context = js_json::from_value(&js_json!({"messages":[]})).unwrap();
    let before = pi_ai::request::now_millis() as f64;
    let mut stream = if method == "stream" {
        api.stream(model, context, StreamOptions::default())
    } else {
        api.stream_simple(model, context, SimpleStreamOptions::default())
    }
    .expect("lazy outer acquisition must succeed");
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    let mut snapshot = js_json!({"callbackCount":calls.load(Ordering::SeqCst),"events":events,"result":stream.result().await.unwrap()});
    mask_timestamps(&mut snapshot, before, pi_ai::request::now_millis() as f64);
    snapshot
}
#[tokio::test]
async fn loaded_module_acquisition_rejections_belong_to_the_frozen_lazy_catch() {
    let mut source = oracle::TsOracle::spawn("provider-lazy-acquisition-oracle.ts", "packages/ai/src/api/lazy.ts");
    let value = JsString::from("lazy-") + JsString::from_utf16(vec![0xd800]);
    for method in ["stream", "streamSimple"] {
        for kind in ["error", "value"] {
            let response = source.request(&js_json!({"method":method,"kind":kind,"value":value,"model":model_value()}));
            assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
            let output = &response["output"];
            let mut expected = output["snapshot"].clone();
            mask_timestamps(
                &mut expected,
                output["clock"]["before"].as_f64().unwrap(),
                output["clock"]["after"].as_f64().unwrap(),
            );
            assert_eq!(expected["callbackCount"], js_json!(1));
            assert_eq!(expected["events"].as_array().unwrap().len(), 1);
            assert_eq!(expected["result"]["errorMessage"], JsValue::String(value.clone()));
            assert_eq!(native_snapshot(method, kind, &value).await, expected, "{method}/{kind}");
        }
    }
}
fn wait_output(mut child: std::process::Child) -> std::process::Output {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() >= deadline {
            let pid = rustix::process::Pid::from_raw(child.id() as i32).unwrap();
            let killed = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            let reaped = child.wait();
            panic!("lazy fixture timeout; kill={killed:?}, reap={reaped:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}
#[test]
fn frozen_lazy_catch_conversion_rejection_is_process_uncaught() {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let mut source = oracle::spawn_oracle_process(
        &lock,
        "provider-lazy-acquisition-oracle.ts",
        "packages/ai/src/api/lazy.ts",
        &[],
        std::process::Stdio::piped(),
    );
    let request =
        js_json::stringify_lossless(&js_json!({"method":"streamSimple","kind":"conversion","model":model_value()}))
            + "\n";
    source.stdin.take().unwrap().write_all(request.as_bytes()).unwrap();
    let source = wait_output(source);
    assert_eq!(source.status.code(), Some(1), "{source:?}");
    assert!(source.stdout.is_empty());
    assert!(
        std::str::from_utf8(&source.stderr)
            .unwrap()
            .contains("TypeError: Cannot convert object to primitive value"),
        "{source:?}"
    );
    let native = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lazy_conversion_child", "--ignored", "--nocapture"])
        .env("PI_LAZY_CONVERSION_CHILD", "1")
        .process_group(0)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let native = wait_output(native);
    assert_eq!(native.status.code(), Some(1), "{native:?}");
    assert_eq!(native.stderr, b"TypeError: Cannot convert object to primitive value\n");
}
#[tokio::test]
#[ignore = "isolated shared process-hook fixture"]
async fn lazy_conversion_child() {
    assert_eq!(std::env::var("PI_LAZY_CONVERSION_CHILD").as_deref(), Ok("1"));
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let _result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        native_snapshot("streamSimple", "conversion", &"unused".into()),
    )
    .await
    .expect("lazy fixture progress");
}

#[test]
fn frozen_lazy_inner_iteration_failure_is_process_uncaught() {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let mut source = oracle::spawn_oracle_process(
        &lock,
        "provider-lazy-acquisition-oracle.ts",
        "packages/ai/src/api/lazy.ts",
        &[],
        std::process::Stdio::piped(),
    );
    let request = js_json::stringify_lossless(
        &js_json!({"method":"stream", "kind":"error", "value":"inner rejected", "inner":true, "model":model_value()}),
    ) + "\n";
    source.stdin.take().unwrap().write_all(request.as_bytes()).unwrap();
    let source = wait_output(source);
    assert_eq!(source.status.code(), Some(1), "{source:?}");
    assert!(source.stdout.is_empty());
    assert!(
        std::str::from_utf8(&source.stderr)
            .unwrap()
            .contains("TypeError: inner rejected"),
        "{source:?}"
    );
    let native = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lazy_inner_failure_child", "--ignored", "--nocapture"])
        .env("PI_LAZY_INNER_CHILD", "1")
        .process_group(0)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let native = wait_output(native);
    assert_eq!(native.status.code(), Some(1), "{native:?}");
    assert_eq!(native.stderr, b"TypeError: inner rejected\n");
}

#[tokio::test]
#[ignore = "isolated shared process-hook fixture"]
async fn lazy_inner_failure_child() {
    assert_eq!(std::env::var("PI_LAZY_INNER_CHILD").as_deref(), Ok("1"));
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let model = js_json::from_value(&model_value()).unwrap();
    let (sender, inner) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
    sender.fail(rejection("error", &"inner rejected".into()).into());
    let mut outer = pi_ai::api::lazy::lazy_stream(model, move || Box::pin(async move { Ok(inner) }));
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), outer.next()).await;
    panic!("iteration failure must exit through shared uncaught authority, got {result:?}");
}
