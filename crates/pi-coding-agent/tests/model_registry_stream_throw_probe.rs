use crate::shared_oracle as oracle;
use pi_ai::request::{CallbackError, SimpleStreamOptions};
use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_registry::{ModelRegistry, ProviderConfigInput, ProviderModelInput},
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

fn definition() -> JsValue {
    js_json!({"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32})
}
fn callback_failure(method: &str, kind: &str, value: &JsString) -> CallbackError {
    if kind == "error" {
        CallbackError::from_rejection(
            NativeNodeError::from_constructor(NativeErrorClass::TypeError, value.clone())
                .with_property("code", JsValue::String("E_STREAM".into()))
                .with_property("detail", js_json!({"method":method}))
                .into(),
        )
    } else {
        CallbackError::from_rejection(AbortReason::Value(JsValue::String(value.clone())))
    }
}
fn rejection_value(error: CallbackError) -> JsValue {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => {
            js_json!({"kind":"error","value":error.value()})
        }
        value => js_json!({"kind":"value","value":value.value()}),
    }
}
fn native_dispatch(method: &str, failure: CallbackError) -> JsValue {
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    registry
        .register_provider(
            "registry-stream-throw",
            ProviderConfigInput {
                api: Some("registry-stream-throw-api".into()),
                base_url: Some("https://fixture.invalid".into()),
                api_key: Some("config-key".into()),
                models: Some(vec![js_json::from_value::<ProviderModelInput>(&definition()).unwrap()]),
                stream_simple: Some(Arc::new(move |_, _, _| {
                    callback_calls.fetch_add(1, Ordering::SeqCst);
                    Err(failure.clone())
                })),
                ..Default::default()
            },
        )
        .unwrap();
    let model = registry.find("registry-stream-throw", "one").unwrap();
    let context = js_json::from_value(&js_json!({"messages":[]})).unwrap();
    let mut options = SimpleStreamOptions::default();
    options.stream.request.api_key = Some("stream-key".into());
    let result = if method == "stream" {
        pi_ai::compat::stream(model, context, options.stream)
    } else {
        pi_ai::compat::stream_simple(model, context, options)
    };
    let rejection = rejection_value(result.err().expect("callback must reject synchronously"));
    registry.unregister_provider("registry-stream-throw").unwrap();
    js_json!({"callbackCount":calls.load(Ordering::SeqCst),"rejection":rejection})
}

#[tokio::test]
async fn registered_raw_and_simple_dispatch_propagate_callback_rejection_like_frozen() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let raw = JsString::from("stream-") + JsString::from_utf16(vec![0xd800]);
    let config = js_json!({"baseUrl":"https://fixture.invalid","models":[definition()]});
    for method in ["stream", "streamSimple"] {
        for kind in ["error", "value"] {
            let result=source.request(&js_json!({"operation":"registration_stream_throw","method":method,"kind":kind,"value":raw,"config":config}));
            assert_eq!(result["ok"], JsValue::Bool(true), "{result:?}");
            let rejection = if kind == "error" {
                js_json!({"kind":"error","value":{"name":"TypeError","message":raw,"code":"E_STREAM","detail":{"method":method}}})
            } else {
                js_json!({"kind":"value","value":raw})
            };
            assert_eq!(
                result["output"],
                js_json!({"callbackCount":1,"rejection":rejection}),
                "{method}/{kind}"
            );
            assert_eq!(
                native_dispatch(method, callback_failure(method, kind, &raw)),
                result["output"],
                "{method}/{kind}"
            );
        }
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
