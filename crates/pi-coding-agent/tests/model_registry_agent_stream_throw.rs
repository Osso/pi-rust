use crate::shared_oracle as oracle;

use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
use pi_ai::request::CallbackError;
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
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn definition() -> JsValue {
    js_json!({"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32})
}

fn failure(kind: &str, value: &JsString) -> CallbackError {
    let reason = match kind {
        "error" => NativeNodeError::from_constructor(NativeErrorClass::TypeError, value.clone())
            .with_property("code", JsValue::String("E_STREAM".into()))
            .with_property("detail", js_json!({"method":"streamSimple"}))
            .into(),
        "conversion" => AbortReason::Value(js_json!({"toString":null,"valueOf":null})),
        "value" => AbortReason::Value(JsValue::String(value.clone())),
        _ => unreachable!("fixture kind"),
    };
    CallbackError::from_rejection(reason)
}

fn normalize_timestamps(value: &mut JsValue, before: f64, after: f64) {
    match value {
        JsValue::Object(object) => {
            if let Some(timestamp) = object.get_mut("timestamp") {
                let actual = timestamp.as_f64().expect("numeric timestamp");
                assert!(
                    (before..=after).contains(&actual),
                    "{actual} outside {before}..={after}"
                );
                *timestamp = js_json!(0);
            }
            for child in object.values_mut() {
                normalize_timestamps(child, before, after);
            }
        }
        JsValue::Array(array) => {
            for child in array {
                normalize_timestamps(child, before, after);
            }
        }
        _ => {}
    }
}

fn register_failure(kind: &str, value: &JsString, provider: &str) -> (ModelRegistry, Arc<AtomicUsize>) {
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let failure = failure(kind, value);
    registry
        .register_provider(
            provider,
            ProviderConfigInput {
                api: Some(format!("{provider}-api")),
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
    (registry, calls)
}

async fn native_snapshot(kind: &str, value: &JsString) -> JsValue {
    let (registry, calls) = register_failure(kind, value, "registry-agent-throw");
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            model: registry.find("registry-agent-throw", "one"),
            ..Default::default()
        }),
        get_api_key: Some(Arc::new(|_| Box::pin(async { Some("stream-key".into()) }))),
        ..Default::default()
    })
    .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    agent.subscribe(Arc::new(move |event, _| {
        observed.lock().unwrap().push(event);
        Box::pin(async { Ok(()) })
    }));
    let before = pi_ai::request::now_millis() as f64;
    let result = match tokio::time::timeout(std::time::Duration::from_secs(5), agent.prompt("prompt"))
        .await
        .expect("prompt settles")
    {
        Ok(()) => js_json!({"kind":"fulfilled"}),
        Err(error) => match CallbackError::from_error(error.message().clone(), error).thrown_value() {
            AbortReason::Error(error) | AbortReason::DomException(error) => {
                js_json!({"kind":"rejected","value":{"name":error.name,"message":error.message}})
            }
            value => js_json!({"kind":"rejected","value":value.value()}),
        },
    };
    let state = agent.state();
    // unregister_provider refreshes and resets every API provider (frozen :381-396).
    // This test owns its API id; retain it until this test process exits so the
    // parallel low-level fixture keeps its independent registrations.
    let mut snapshot = js_json!({"callbackCount":calls.load(Ordering::SeqCst),"result":result,"events":events.lock().unwrap().clone(),"state":{
        "messages":state.messages,"isStreaming":state.is_streaming,"isModelRequestActive":state.is_model_request_active,
        "streamingMessage":state.streaming_message,"pendingToolCalls":state.pending_tool_calls,"errorMessage":state.error_message
    }});
    normalize_timestamps(&mut snapshot, before, pi_ai::request::now_millis() as f64);
    snapshot
}

#[tokio::test]
async fn registered_throw_reaches_default_agent_catch_with_exact_events_and_state() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let value = JsString::from("agent-") + JsString::from_utf16(vec![0xd800]);
    let mut actual_cases = Vec::new();
    let mut expected_cases = Vec::new();
    for kind in ["error", "value", "conversion"] {
        let response = source.request(&js_json!({"operation":"registration_agent_throw","kind":kind,"value":value,"config":{"baseUrl":"https://fixture.invalid","models":[definition()]}}));
        assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
        let output = &response["output"];
        let mut expected = output["snapshot"].clone();
        normalize_timestamps(
            &mut expected,
            output["clock"]["before"].as_f64().unwrap(),
            output["clock"]["after"].as_f64().unwrap(),
        );
        assert_eq!(expected["callbackCount"], js_json!(1));
        assert_eq!(expected["state"]["isStreaming"], JsValue::Bool(false));
        if kind != "conversion" {
            assert_eq!(expected["result"], js_json!({"kind":"fulfilled"}));
            assert_eq!(expected["state"]["errorMessage"], JsValue::String(value.clone()));
        } else {
            assert_eq!(
                expected["result"],
                js_json!({"kind":"rejected","value":{"name":"TypeError","message":"Cannot convert object to primitive value"}})
            );
        }
        actual_cases.push((kind, native_snapshot(kind, &value).await));
        expected_cases.push((kind, expected));
    }
    assert_eq!(actual_cases, expected_cases);
}

fn stream_rejection(error: pi_ai::utils::event_stream::StreamFailure) -> JsValue {
    match CallbackError::from_error(error.message().clone(), error).thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => {
            js_json!({"kind":"error","value":error.value()})
        }
        value => js_json!({"kind":"value","value":value.value()}),
    }
}

async fn native_loop_snapshot(consumer: &str, kind: &str, value: &JsString) -> JsValue {
    use pi_agent_core::agent::{AgentContext, AgentLoopConfig, AgentMessage, agent_loop, agent_loop_continue};
    let provider = format!("registry-agent-{consumer}-throw");
    let (registry, calls) = register_failure(kind, value, &provider);
    let before = pi_ai::request::now_millis() as f64;
    let prompt: AgentMessage =
        js_json::from_value(&js_json!({"role":"user","content":[{"type":"text","text":"prompt"}],"timestamp":before}))
            .unwrap();
    let context = AgentContext {
        system_prompt: "".into(),
        messages: Vec::new(),
        tools: Some(Vec::new()),
    };
    let mut config = AgentLoopConfig::new(
        registry.find(&provider, "one").unwrap(),
        Arc::new(|messages| {
            Box::pin(async move {
                Ok(messages
                    .into_iter()
                    .map(|message| match message {
                        AgentMessage::User(user) => pi_ai::types::Message::User(user),
                        _ => panic!("fixture contains only user input"),
                    })
                    .collect())
            })
        }),
    );
    config.stream_options.stream.request.api_key = Some("stream-key".into());
    let mut stream = if consumer == "loop" {
        agent_loop(vec![prompt], context, config, None, None)
    } else {
        agent_loop_continue(
            AgentContext {
                messages: vec![prompt],
                ..context
            },
            config,
            None,
            None,
        )
        .unwrap()
    };
    let mut events = Vec::new();
    let mut iteration = JsValue::Null;
    while let Some(event) = stream.next().await {
        match event {
            Ok(event) => events.push(event),
            Err(error) => {
                iteration = stream_rejection(error);
                break;
            }
        }
    }
    let result = stream_rejection(stream.result().await.expect_err("acquisition must fail"));
    // Do not reset the process-global API registry while the Agent test runs.
    // Loop and continue each own a distinct API id until process exit.
    let mut snapshot =
        js_json!({"callbackCount":calls.load(Ordering::SeqCst),"events":events,"iteration":iteration,"result":result});
    normalize_timestamps(&mut snapshot, before, pi_ai::request::now_millis() as f64);
    snapshot
}

#[tokio::test]
async fn low_level_default_loops_preserve_registered_rejection_without_text_conversion() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let value = JsString::from("loop-") + JsString::from_utf16(vec![0xd800]);
    let mut actual_cases = Vec::new();
    let mut expected_cases = Vec::new();
    for consumer in ["loop", "continue"] {
        for kind in ["error", "value", "conversion"] {
            let response = source.request(&js_json!({"operation":"registration_agent_throw","consumer":consumer,"kind":kind,"value":value,"config":{"baseUrl":"https://fixture.invalid","models":[definition()]}}));
            assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
            let output = &response["output"];
            let mut expected = output["snapshot"].clone();
            normalize_timestamps(
                &mut expected,
                output["clock"]["before"].as_f64().unwrap(),
                output["clock"]["after"].as_f64().unwrap(),
            );
            assert_eq!(expected["callbackCount"], js_json!(1));
            assert_eq!(expected["iteration"], expected["result"]);
            let actual = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                native_loop_snapshot(consumer, kind, &value),
            )
            .await
            .expect("loop settles");
            actual_cases.push((consumer, kind, actual));
            expected_cases.push((consumer, kind, expected));
        }
    }
    assert_eq!(actual_cases, expected_cases);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
