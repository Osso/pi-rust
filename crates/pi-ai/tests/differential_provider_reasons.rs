use crate::shared_abort_reason as abort_reason;
use crate::shared_oracle as oracle;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, StreamOptions};
use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal};
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{Arc, Mutex};

fn decode(input: &JsValue) -> Option<AbortReason> {
    if input["kind"] == JsValue::String("default".into()) {
        return None;
    }
    if input["kind"] == JsValue::String("value".into()) {
        return Some(AbortReason::Value(input["value"].clone()));
    }
    let message = input["message"].as_string().unwrap().clone();
    let mut error = if input["kind"] == JsValue::String("type-error".into()) {
        pi_js_compat::node_error::NativeNodeError::from_constructor(
            pi_js_compat::node_error::NativeErrorClass::TypeError,
            message,
        )
    } else {
        pi_js_compat::node_error::NativeNodeError::new(
            input["name"].as_string().cloned().unwrap_or_else(|| "Error".into()),
            message,
        )
    };
    if let Some(properties) = input["properties"].as_object() {
        for (name, value) in properties {
            error = error.with_property(name.clone(), value.clone());
        }
    }
    Some(if input["kind"] == JsValue::String("dom".into()) {
        AbortReason::DomException(error)
    } else {
        AbortReason::Error(error)
    })
}
struct ScriptBody(Option<CallbackError>);
impl HttpBody for ScriptBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let error = self.0.take();
        Box::pin(async move { error.map(Err) })
    }
}
struct PendingSignalBody(AbortSignal);
impl HttpBody for PendingSignalBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move { Some(Err(CallbackError::from_rejection(self.0.wait().await))) })
    }
}
struct ReasonFetch {
    input: JsValue,
    caller: AbortController,
    signals: Mutex<Vec<Option<AbortSignal>>>,
}
impl Fetch for ReasonFetch {
    fn fetch(
        &self,
        _request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.signals.lock().unwrap().push(signal.clone());
        Box::pin(async move {
            let mode = self.input["mode"].as_string().unwrap().to_utf8().unwrap();
            if mode == "reject" {
                return Err(CallbackError::from_rejection(
                    decode(&self.input["failure"]).expect("failure value"),
                ));
            }
            if mode == "body-timeout" {
                return Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: vec![("content-type".into(), "text/event-stream".into())],
                    body: Box::new(PendingSignalBody(signal.expect("deadline signal"))),
                });
            }
            if mode == "body" || mode == "success" {
                let error = (mode == "body")
                    .then(|| CallbackError::from_rejection(decode(&self.input["failure"]).expect("failure value")));
                return Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: vec![("content-type".into(), "text/event-stream".into())],
                    body: Box::new(ScriptBody(error)),
                });
            }
            if mode == "live" {
                self.caller.abort(decode(&self.input["failure"]));
            }
            Err(CallbackError::from_rejection(
                signal.expect("request signal").wait().await,
            ))
        })
    }
}
async fn observe(
    provider: &str,
    input: &JsValue,
    model: pi_ai::types::Model,
    context: pi_ai::types::Context,
) -> JsValue {
    let caller = AbortController::new();
    if input["mode"] == JsValue::String("pre".into()) {
        caller.abort(decode(&input["failure"]));
    }
    let signal = (input["caller"] != JsValue::Bool(false)).then(|| caller.signal());
    let fetch = Arc::new(ReasonFetch {
        input: input.clone(),
        caller: caller.clone(),
        signals: Mutex::new(Vec::new()),
    });
    let mut options = StreamOptions::default();
    options.request.api_key = Some("token".into());
    options.request.signal = signal;
    options.request.fetch = Some(fetch.clone());
    options.request.timeout_ms = input["timeoutMs"].as_f64();
    options.request.max_retries = Some(0.0);
    let mut stream = if provider == "google" {
        pi_ai::api::google_generative_ai::stream(model, context, options)
    } else {
        pi_ai::api::mistral_conversations::stream(
            model,
            context,
            pi_ai::api::mistral_conversations::MistralOptions {
                stream: options,
                ..Default::default()
            },
        )
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(61), async {
        while stream.next().await.is_some() {}
        stream.result().await.expect("provider message")
    })
    .await;
    if input["abortAfter"] == JsValue::Bool(true) {
        caller.abort(decode(&input["failure"]));
    }
    let signals = fetch.signals.lock().unwrap();
    let observed = signals.first().cloned().flatten();
    let (stop_reason, error_message) = match result {
        Ok(message) => (
            pi_js_compat::js_json::to_value(&message.stop_reason).unwrap(),
            message.error_message.map(JsValue::String).unwrap_or(JsValue::Null),
        ),
        Err(_) => (
            JsValue::String("error".into()),
            JsValue::String("test harness deadline".into()),
        ),
    };
    pi_js_compat::js_json!({"stopReason":stop_reason,"errorMessage":error_message,"invoked":!signals.is_empty(),"signal":observed.map(|signal|pi_js_compat::js_json!({"aborted":signal.aborted(),"reason":abort_reason::describe(signal.reason())})).unwrap_or(JsValue::Null)})
}

#[tokio::test(start_paused = true)]
async fn google_and_mistral_reason_boundaries_match_frozen_whole_providers() {
    let cases = pi_js_compat::js_json::parse_lossless(
        r#"[
        {"mode":"success"},
        {"mode":"success","caller":false},
        {"mode":"success","abortAfter":true,"failure":{"kind":"error","name":"CallerError","message":"native\ud800"}},
        {"mode":"pre","failure":{"kind":"error","name":"CallerError","message":"native\ud800"}},
        {"mode":"live","failure":{"kind":"default"}},
        {"mode":"live","failure":{"kind":"error","name":"CallerError","message":"native\ud800"}},
        {"mode":"live","failure":{"kind":"value","value":false}},
        {"mode":"reject","failure":{"kind":"error","name":"TimeoutError","message":"deadline\ud800"}},
        {"mode":"reject","failure":{"kind":"error","name":"TypeError","message":"fetch failed"}},
        {"mode":"reject","failure":{"kind":"type-error","name":"TypeError","message":"fetch failed"}},
        {"mode":"reject","failure":{"kind":"value","value":{"name":"AbortError","message":"lookalike"}}},
        {"mode":"reject","failure":{"kind":"value","value":"raw\ud800"}},
        {"mode":"reject","failure":{"kind":"value","value":false}},
        {"mode":"body","failure":{"kind":"error","name":"CallerError","message":"body\ud800"}},
        {"mode":"body","failure":{"kind":"value","value":{"name":"AbortError","message":"lookalike"}}},
        {"mode":"body","failure":{"kind":"value","value":null}},
        {"mode":"reject","failure":{"kind":"value","value":{"code":20}}},
        {"mode":"reject","failure":{"kind":"value","value":{"code":23}}},
        {"mode":"reject","failure":{"kind":"value","value":{"code":"ECONNABORTED"}}},
        {"mode":"reject","failure":{"kind":"value","value":{"code":"ECONNRESET"}}},
        {"mode":"reject","failure":{"kind":"value","value":{"name":"ConnectionError"}}},
        {"mode":"reject","failure":{"kind":"value","value":{"toString":false}}},
        {"mode":"reject","failure":{"kind":"value","value":null}},
        {"mode":"reject","failure":{"kind":"value","value":0}},
        {"mode":"reject","failure":{"kind":"value","value":""}},
        {"mode":"reject","failure":{"kind":"value","value":[]}},
        {"mode":"reject","failure":{"kind":"type-error","message":"FETCH FAILED \ud800","properties":{"name":"renamed"}}}
    ]"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for provider in ["google", "mistral"] {
        let source = if provider == "google" {
            "packages/ai/src/api/google-generative-ai.ts"
        } else {
            "packages/ai/src/api/mistral-conversations.ts"
        };
        let mut oracle = oracle::TsOracle::spawn("provider-reasons-oracle.ts", source);
        let api = if provider == "google" {
            "google-generative-ai"
        } else {
            "mistral-conversations"
        };
        let model = pi_js_compat::js_json!({"id":"test","name":"test","api":api,"provider":provider,"baseUrl":"https://example.test","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":100,"maxTokens":10});
        let context = pi_js_compat::js_json!({"messages":[{"role":"user","content":"Hi","timestamp":1}]});
        let mut provider_cases = cases.as_array().unwrap().clone();
        if provider == "mistral" {
            provider_cases.push(pi_js_compat::js_json!({"mode":"timeout","caller":false,"timeoutMs":5}));
            provider_cases.push(pi_js_compat::js_json!({"mode":"body-timeout","caller":false,"timeoutMs":5}));
        }
        for case in &provider_cases {
            let mut input = case.clone();
            input
                .as_object_mut()
                .unwrap()
                .insert("id".into(), JsValue::String("provider-reasons".into()));
            input.as_object_mut().unwrap().insert("model".into(), model.clone());
            input.as_object_mut().unwrap().insert("context".into(), context.clone());
            let source = oracle.request(&input);
            assert_eq!(source["ok"], JsValue::Bool(true), "{source:?}");
            let before = tokio::time::Instant::now();
            let actual = observe(
                provider,
                &input,
                pi_js_compat::js_json::from_value(&model).unwrap(),
                pi_js_compat::js_json::from_value(&context).unwrap(),
            )
            .await;
            if input["mode"] == JsValue::String("timeout".into())
                || input["mode"] == JsValue::String("body-timeout".into())
            {
                assert_eq!(source["output"]["delays"], pi_js_compat::js_json!([60000]));
                assert_eq!(before.elapsed(), std::time::Duration::from_secs(60));
            }
            let mut expected = source["output"].clone();
            expected
                .as_object_mut()
                .unwrap()
                .shift_remove(&JsString::from("delays"));
            if actual != expected {
                failures.push(format!(
                    "{provider} case={} actual={} source={}",
                    pi_js_compat::js_json::stringify_lossless(case),
                    pi_js_compat::js_json::stringify_lossless(&actual),
                    pi_js_compat::js_json::stringify_lossless(&expected)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
