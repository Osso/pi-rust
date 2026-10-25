use super::oracle;
use pi_ai::{
    compat::{ApiProvider, register_api_provider, unregister_api_providers},
    request::{CallbackError, now_millis},
    types::{AssistantMessage, Message},
    utils::event_stream::create_assistant_message_event_stream,
};
use pi_coding_agent::supervisor::main::create_supervisor_instruction_evaluator;
use pi_js_compat::{
    abort::AbortController,
    js_json as json,
    js_json::{JsObject, JsString, JsValue},
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct Script {
    responses: VecDeque<JsValue>,
    calls: Vec<JsValue>,
    controller: AbortController,
    started: i64,
    abort_during_stream: bool,
}
pub(super) fn write_models(root: &std::path::Path) {
    let mut providers = JsObject::new();
    let mut auth = JsObject::new();
    for provider in ["openai-codex", "openai-codex-gc", "openai-codex-team"] {
        auth.insert(
            provider.into(),
            json!({"type":"api_key","key":format!("fixture-{provider}")}),
        );
        providers.insert(provider.into(),json!({
            "baseUrl":"https://unused.invalid","api":"supervisor-review-fixture","headers":{"X-Review":"fixture"},
            "models":[{"id":"gpt-6.1-sol","name":"Sol","reasoning":true,"input":["text"],
                "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":200000,"maxTokens":8192}],
        }));
    }
    std::fs::write(
        root.join("auth.json"),
        pi_js_compat::js_json::stringify_lossless(&JsValue::Object(auth)),
    )
    .unwrap();
    std::fs::write(
        root.join("models.json"),
        pi_js_compat::js_json::stringify_lossless(&JsValue::object([("providers", JsValue::Object(providers))])),
    )
    .unwrap();
}
fn register_fixture(script: Arc<Mutex<Script>>) {
    register_api_provider(
        ApiProvider::new(
            "supervisor-review-fixture",
            |_, _, _| Err(CallbackError::new("unexpected raw stream")),
            move |model, mut context, options| {
                let mut script = script.lock().unwrap();
                for message in &mut context.messages {
                    let Message::User(user) = message else {
                        panic!("expected user message")
                    };
                    assert!(user.timestamp >= script.started as f64 && user.timestamp <= now_millis() as f64);
                    user.timestamp = 0.0;
                }
                script.calls.push(json!({"model":{"id":model.id,"provider":model.provider,"api":model.api},"context":context,"options":{
                "apiKey":options.stream.request.api_key,"env":options.stream.request.env,"headers":options.stream.request.headers,
                "maxRetries":options.stream.request.max_retries,"maxRetryDelayMs":options.stream.request.max_retry_delay_ms,
                "reasoning":options.reasoning,"timeoutMs":options.stream.request.timeout_ms,
                "websocketConnectTimeoutMs":options.stream.websocket_connect_timeout_ms,
                "signal":{"present":options.stream.request.signal.is_some(),"aborted":options.stream.request.signal.as_ref().map(|signal|signal.aborted())},
            }}));
                let response = script.responses.pop_front().expect("unexpected fixture provider call");
                if let Some(error) = response["throw"].as_string() {
                    return Err(CallbackError::new(error.clone()));
                }
                if script.abort_during_stream {
                    script.controller.abort(None);
                }
                let mut message = AssistantMessage::empty(&model.api, &model.provider, &model.id, 0.0);
                message.stop_reason = pi_js_compat::js_json::from_value(&response["stopReason"]).unwrap();
                message.content = pi_js_compat::js_json::from_value(&response["content"]).unwrap();
                message.error_message = response["errorMessage"].as_string().cloned();
                let (sender, stream) = create_assistant_message_event_stream();
                sender.end(Some(message));
                Ok(stream)
            },
        ),
        Some("supervisor-review-test".into()),
    );
}
#[tokio::test]
async fn stateless_instruction_evaluation_matches_frozen_provider_contract() {
    let directory = tempfile::tempdir().unwrap();
    write_models(directory.path());
    let agent_dir = JsString::from(directory.path().to_str().unwrap());
    let script = Arc::new(Mutex::new(Script {
        responses: VecDeque::new(),
        calls: Vec::new(),
        controller: AbortController::new(),
        started: now_millis(),
        abort_during_stream: false,
    }));
    register_fixture(script.clone());
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-supervisor-main-oracle.ts",
        "packages/coding-agent/src/supervisor/main.ts",
    );
    let accepted = json!({"stopReason":"stop","content":[{"type":"thinking","thinking":"unrelated"},{"type":"text","text":"AC"},{"type":"text","text":"CEPT"}]});
    let quota = json!({"stopReason":"error","content":[],"errorMessage":"usage limit"});
    let cases = [
        (json!([accepted]), false, false),
        (json!([quota, accepted]), false, false),
        (json!([quota, quota, quota]), false, false),
        (
            json!([{"stopReason":"error","content":[],"errorMessage":"connection failed"}]),
            false,
            false,
        ),
        (
            json!([{"stopReason":"length","content":[{"type":"text","text":"ACCEPT"}]}]),
            false,
            false,
        ),
        (json!([{"stopReason":"stop","content":[]}]), false, false),
        (
            json!([{"stopReason":"stop","content":[{"type":"text","text":" ACCEPT "}]}]),
            false,
            false,
        ),
        (json!([{"throw":"stream failed"}]), false, false),
        (json!([]), true, false),
        (json!([accepted]), false, true),
    ];
    for (responses, aborted, abort_during_stream) in cases {
        let expected=source.request(&json!({"id":1,"operation":"evaluateInstructions","agentDir":agent_dir,
            "instructions":"Inspect evidence.","responses":responses,"aborted":aborted,"abortDuringStream":abort_during_stream,
        }));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let controller = AbortController::new();
        if aborted {
            controller.abort(None);
        }
        {
            let mut script = script.lock().unwrap();
            script.responses = responses.as_array().unwrap().iter().cloned().collect();
            script.calls.clear();
            script.started = now_millis();
            script.controller = controller.clone();
            script.abort_during_stream = abort_during_stream;
        }
        let evaluate = create_supervisor_instruction_evaluator(&agent_dir).unwrap();
        let result = evaluate("Inspect evidence.".into(), controller.signal()).await;
        let calls = script.lock().unwrap().calls.clone();
        let actual = match result {
            Ok(result) => json!({"result":result,"error":null,"calls":calls}),
            Err(error) => json!({"result":null,"error":error,"calls":calls}),
        };
        assert_eq!(actual, expected["output"]);
    }
    unregister_api_providers("supervisor-review-test");
}
