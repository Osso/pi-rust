use crate::shared_abort_reason as abort_reason;
use crate::shared_oracle as oracle;

use indexmap::IndexMap;
use pi_ai::api::{anthropic_sdk, openai_sdk};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse};
use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal};
use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};

struct EmptyBody;
impl pi_ai::request::HttpBody for EmptyBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { None })
    }
}

struct SdkFetch {
    input: JsValue,
    caller: AbortController,
    signals: Mutex<Vec<AbortSignal>>,
}
impl Fetch for SdkFetch {
    fn fetch(
        &self,
        _request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let signal = signal.expect("SDK fetch signal");
        self.signals.lock().unwrap().push(signal.clone());
        Box::pin(async move {
            if self.input["mode"] == JsValue::String("success".into()) {
                return Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: Vec::new(),
                    body: Box::new(EmptyBody),
                });
            }
            if self.input["mode"] == JsValue::String("reject".into()) {
                let failure = &self.input["failure"];
                let rejection = if failure["kind"] == JsValue::String("value".into()) {
                    AbortReason::Value(failure["value"].clone())
                } else {
                    let mut error = pi_js_compat::node_error::NativeNodeError::new(
                        failure["name"].as_string().cloned().unwrap_or_else(|| "Error".into()),
                        failure["message"].as_string().unwrap().clone(),
                    );
                    if let Some(properties) = failure["properties"].as_object() {
                        for (name, value) in properties {
                            error = error.with_property(name.clone(), value.clone());
                        }
                    }
                    if failure["kind"] == JsValue::String("dom".into()) {
                        AbortReason::DomException(error)
                    } else {
                        AbortReason::Error(error)
                    }
                };
                return Err(CallbackError::from_rejection(rejection));
            }
            if self.input["mode"] == JsValue::String("caller".into()) {
                self.caller.abort(Some(AbortReason::Value(JsValue::String(
                    pi_js_compat::js_json::JsString::from_utf16("native".encode_utf16().chain([0xd800]).collect()),
                ))));
            }
            Err(CallbackError::from_rejection(signal.wait().await))
        })
    }
}

async fn execute(
    sdk: &str,
    fetch: Arc<SdkFetch>,
    signal: Option<AbortSignal>,
    timeout: Option<f64>,
) -> Option<pi_js_compat::js_json::JsString> {
    if sdk == "anthropic" {
        let model: pi_ai::types::Model = serde_json::from_value(serde_json::json!({
            "id":"test","name":"test","api":"anthropic-messages","provider":"anthropic","baseUrl":"http://sdk.invalid",
            "reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":100,"maxTokens":1
        })).unwrap();
        let request = anthropic_sdk::build_request(
            &model,
            &serde_json::json!({"model":"test","max_tokens":1,"messages":[],"stream":true}),
            anthropic_sdk::Auth::ApiKey("token".into()),
            &IndexMap::new(),
            timeout,
        )
        .unwrap();
        anthropic_sdk::execute(&request, fetch, signal)
            .await
            .err()
            .map(|error| openai_sdk::format_request_error(&error, None))
    } else {
        let client = openai_sdk::SdkClient::new(openai_sdk::SdkClientConfig {
            api_key: "token".into(),
            base_url: "http://sdk.invalid".into(),
            default_headers: IndexMap::new(),
            timeout_ms: timeout,
            fetch: Some(fetch),
            azure: None,
        });
        client
            .create_response_stream(
                pi_js_compat::js_json!({"model":"test","input":[],"stream":true}),
                signal,
            )
            .await
            .err()
            .map(|error| openai_sdk::format_request_error(&error, None))
    }
}

#[tokio::test(start_paused = true)]
async fn stainless_sdk_timeout_reason_and_caller_lifetimes_match_pinned_sdks() {
    let mut oracle = oracle::TsOracle::spawn("stainless-abort-oracle.ts", "packages/ai/src/api/openai-responses.ts");
    let cases = pi_js_compat::js_json::parse_lossless(r#"[
        {"mode":"timeout","timeoutMs":5},
        {"mode":"caller"},
        {"mode":"success"},
        {"mode":"success","abortAfter":true},
        {"mode":"success","caller":false},
        {"mode":"success","timeoutMs":5,"waitAfter":true},
        {"mode":"success","preAbort":true},
        {"mode":"reject","failure":{"kind":"error","name":"AbortError","message":"stop"}},
        {"mode":"reject","failure":{"kind":"dom","name":"TimeoutError","message":"deadline expired"}},
        {"mode":"reject","failure":{"kind":"error","name":"NetworkError","message":"connect timed out"}},
        {"mode":"reject","failure":{"kind":"error","message":"FetchRequestCanceledException"}},
        {"mode":"reject","failure":{"kind":"value","value":"timed out"}},
        {"mode":"reject","failure":{"kind":"value","value":{"name":"AbortError","message":"stop"}}},
        {"mode":"reject","failure":{"kind":"error","message":"fetch failed","properties":{"cause":"connect timed out"}}},
        {"mode":"reject","failure":{"kind":"error","message":"fetch failed","properties":{"cause":{"toString":false}}}},
        {"mode":"reject","failure":{"kind":"error","name":"timed out","message":""}},
        {"mode":"reject","failure":{"kind":"error","name":"","message":"time out"}},
        {"mode":"reject","failure":{"kind":"value","value":{"name":"NotError","message":"timed out"}}},
        {"mode":"reject","failure":{"kind":"value","value":null}}
    ]"#).unwrap();
    let mut failures = Vec::new();
    for sdk in ["openai", "anthropic"] {
        for input in cases.as_array().unwrap() {
            let mut request = input.clone();
            let object = request.as_object_mut().unwrap();
            object.insert("id".into(), JsValue::String("sdk-abort".into()));
            object.insert("sdk".into(), JsValue::String(sdk.into()));
            let expected = oracle.request(&request);
            assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
            let caller = AbortController::new();
            if input["preAbort"] == JsValue::Bool(true) {
                caller.abort(Some(AbortReason::Value(JsValue::String("pre".into()))));
            }
            let signal = (input["caller"] != JsValue::Bool(false)).then(|| caller.signal());
            let fetch = Arc::new(SdkFetch {
                input: input.clone(),
                caller: caller.clone(),
                signals: Mutex::new(Vec::new()),
            });
            let outcome = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                execute(sdk, fetch.clone(), signal.clone(), input["timeoutMs"].as_f64()),
            )
            .await;
            let error = outcome.unwrap_or_else(|_| Some("test harness deadline elapsed".into()));
            if input["waitAfter"] == JsValue::Bool(true) {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            if input["abortAfter"] == JsValue::Bool(true) {
                caller.abort(Some(AbortReason::Value(JsValue::String("after".into()))));
            }
            let observed = fetch.signals.lock().unwrap().first().cloned();
            let actual = pi_js_compat::js_json!({
                "message":error.map(JsValue::String).unwrap_or(JsValue::Null),
                "sameAsCaller":signal.as_ref().zip(observed.as_ref()).is_some_and(|(signal,observed)|signal==observed),
                "fetchSignal":observed.map(|observed|pi_js_compat::js_json!({"aborted":observed.aborted(),"reason":abort_reason::describe(observed.reason())})).unwrap_or(JsValue::Null)
            });
            let expected = pi_js_compat::js_json!({"message":expected["output"]["error"].get("message").cloned().unwrap_or(JsValue::Null),"sameAsCaller":expected["output"]["sameAsCaller"],"fetchSignal":expected["output"]["fetchSignal"]});
            if actual != expected {
                failures.push(format!(
                    "{sdk} input={} actual={} source={}",
                    pi_js_compat::js_json::stringify_lossless(input),
                    pi_js_compat::js_json::stringify_lossless(&actual),
                    pi_js_compat::js_json::stringify_lossless(&expected)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
