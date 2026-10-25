#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::api::openrouter_images::generate_images;
use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ImagesOptions, ImagesRequestOptions,
};
use pi_ai::types::{ImagesContext, ImagesModel};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};

fn model_from_json(value: &Value) -> ImagesModel {
    serde_json::from_value(value.clone()).expect("fixture model deserializes")
}

fn context_from_json(value: &Value) -> ImagesContext {
    serde_json::from_value(value.clone()).expect("fixture context deserializes")
}

fn headers_from_json(value: &Value) -> Option<indexmap::IndexMap<String, Option<pi_js_compat::js_json::JsString>>> {
    value.as_object().map(|object| {
        object
            .iter()
            .map(|(name, value)| (name.clone(), value.as_str().map(pi_js_compat::js_json::JsString::from)))
            .collect()
    })
}

fn normalize_headers(headers: &[(String, String)]) -> Value {
    let mut record = Map::new();
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        if name == "user-agent" || (name.starts_with("x-stainless-") && name != "x-stainless-retry-count") {
            continue;
        }
        record.insert(name, Value::String(value.clone()));
    }
    Value::Object(record)
}

fn observed(request: &Value) -> Value {
    let model = model_from_json(&request["model"]);
    let context = context_from_json(&request["context"]);
    let option_values = request["options"].as_object().expect("options object");
    let response = &option_values["response"];
    let fixture = FixtureResponse {
        status: u16::try_from(response["status"].as_u64().expect("status")).expect("status fits"),
        headers: response["headers"]
            .as_object()
            .map(|headers| {
                headers
                    .iter()
                    .map(|(name, value)| (name.clone(), value.as_str().expect("header string").to_owned()))
                    .collect()
            })
            .unwrap_or_default(),
        body: response["body"].as_str().expect("body string").to_owned(),
        body_delay_ms: response.get("bodyDelayMs").and_then(Value::as_u64),
    };
    let fetch = Arc::new(FixtureFetch {
        response: fixture,
        requests: Mutex::new(Vec::new()),
    });
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    if option_values.get("aborted").and_then(Value::as_bool).unwrap_or(false) {
        signal_controller.abort(None);
    }
    let payload_error = option_values.get("payloadError").cloned();
    let replacement = option_values.get("replacePayload").cloned();
    let response_error = option_values.get("responseError").cloned();
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            on_payload: Some(Arc::new(move |_, _| {
                let error = payload_error.clone();
                let replacement = replacement.clone();
                Box::pin(async move {
                    if let Some(error) = error {
                        return Err(CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(
                            error.into(),
                        )));
                    }
                    Ok(replacement.map(Into::into))
                })
            })),
            on_response: Some(Arc::new(move |_, _| {
                let error = response_error.clone();
                Box::pin(async move {
                    if let Some(error) = error {
                        return Err(CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(
                            error.into(),
                        )));
                    }
                    Ok(())
                })
            })),
            api_key: (option_values
                .get("apiKey")
                .and_then(Value::as_str)
                .map(pi_js_compat::js_json::JsString::from)),
            headers: option_values.get("headers").and_then(headers_from_json),
            fetch: Some(fetch.clone()),
            signal: Some(signal),
            max_retries: option_values.get("maxRetries").and_then(Value::as_f64),
            timeout_ms: option_values.get("timeoutMs").and_then(Value::as_f64),
            ..Default::default()
        },
        ..Default::default()
    };

    let output = block_on(generate_images(model, context, options)).expect("generation returns a result");
    let requests = fetch.requests.lock().expect("requests lock").clone();
    let recorded = match requests.last() {
        Some(request) => json!({
            "method": request.method,
            "url": request.url,
            "headers": normalize_headers(&request.headers),
            "body": request.body.as_deref().map(|body| String::from_utf8_lossy(body).into_owned()),
        }),
        None => json!({"method": "", "url": "", "headers": {}, "body": null}),
    };
    let mut output = serde_json::to_value(output).expect("output serializes");
    output["timestamp"] = json!(0);
    json!({
        "id": request["id"],
        "ok": true,
        "request": recorded,
        "output": output,
    })
}

struct FixtureResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    body_delay_ms: Option<u64>,
}

struct FixtureFetch {
    response: FixtureResponse,
    requests: Mutex<Vec<HttpRequest>>,
}

impl Fetch for FixtureFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().expect("requests lock").push(request);
        let response = HttpResponse {
            status: self.response.status,
            status_text: String::new(),
            headers: self.response.headers.clone(),
            body: Box::new(BytesBody {
                chunk: Some(self.response.body.as_bytes().to_vec()),
                delay_ms: self.response.body_delay_ms,
            }),
        };
        Box::pin(async move { Ok(response) })
    }
}

struct BytesBody {
    chunk: Option<Vec<u8>>,
    delay_ms: Option<u64>,
}

impl HttpBody for BytesBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.chunk.take();
        let delay = self.delay_ms.take();
        Box::pin(async move {
            if let Some(delay) = delay {
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            chunk.map(Ok)
        })
    }
}

fn block_on<T>(future: pi_ai::request::BoxFuture<'static, T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

fn base_model() -> Value {
    json!({
        "id": "google/gemini-3.1-flash-image-preview",
        "name": "Gemini 3.1 Flash Image Preview",
        "api": "openrouter-images",
        "provider": "openrouter",
        "baseUrl": "https://openrouter.ai/api/v1",
        "input": ["text", "image"],
        "output": ["text", "image"],
        "cost": {"input": 0.015, "output": 0.03, "cacheRead": 0.003, "cacheWrite": 0.01},
        "headers": {"HTTP-Referer": "https://example.com"}
    })
}

fn success_body() -> String {
    json!({
        "id": "img-1",
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 34,
            "prompt_tokens_details": {"cached_tokens": 0}
        },
        "choices": [{
            "message": {
                "content": "Here is your image.",
                "images": [
                    {"image_url": "data:image/png;base64,ZmFrZS1wbmc="},
                    {"image_url": {"url": "data:image/jpeg;base64,AAAA"}},
                    {"image_url": "https://example.com/x.png"},
                    {"image_url": "data:image/png;base64,"},
                    {"image_url": {"url": "data:image/png"}}
                ]
            }
        }]
    })
    .to_string()
}

fn fixtures() -> Vec<Value> {
    let text_context = json!({"input": [{"type": "text", "text": "Generate a dog"}]});
    let image_context = json!({"input": [
        {"type": "text", "text": "Make it blue"},
        {"type": "image", "data": "aGk=", "mimeType": "image/png"}
    ]});
    let mut cases = vec![
        json!({
            "id": 0,
            "model": base_model(),
            "context": text_context,
            "options": {
                "apiKey": "test",
                "headers": {"X-Extra": "1"},
                "response": {"status": 200, "headers": {"content-type": "application/json", "x-request-id": "r1"}, "body": success_body()}
            }
        }),
        json!({
            "id": 1,
            "model": base_model(),
            "context": image_context,
            "options": {
                "apiKey": "test",
                "response": {"status": 200, "headers": {"content-type": "application/json"}, "body": success_body()}
            }
        }),
        json!({
            "id": 2,
            "model": {"id": "flux", "name": "Flux", "api": "openrouter-images", "provider": "openrouter", "baseUrl": "https://openrouter.ai/api/v1/", "input": ["text"], "output": ["image"], "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}},
            "context": text_context,
            "options": {
                "apiKey": "test",
                "response": {"status": 200, "headers": {"content-type": "application/json"}, "body": json!({"usage": {"prompt_tokens": 100, "completion_tokens": 10, "prompt_tokens_details": {"cached_tokens": 40, "cache_write_tokens": 10}}, "choices": []}).to_string()}
            }
        }),
        json!({
            "id": 3,
            "model": base_model(),
            "context": text_context,
            "options": {"apiKey": "test", "aborted": true, "response": {"status": 200, "headers": {}, "body": "{}"}}
        }),
        json!({
            "id": 4,
            "model": base_model(),
            "context": text_context,
            "options": {"apiKey": "test", "response": {"status": 400, "headers": json!({"content-type": "application/json"}), "body": json!({"error": {"message": "bad request"}}).to_string()}}
        }),
        json!({
            "id": 5,
            "model": base_model(),
            "context": text_context,
            "options": {"apiKey": "test", "response": {"status": 500, "headers": {"content-type": "text/plain"}, "body": "boom"}}
        }),
        json!({
            "id": 6,
            "model": base_model(),
            "context": text_context,
            "options": {"apiKey": "test", "response": {"status": 429, "headers": json!({"content-type": "application/json"}), "body": ""}}
        }),
        json!({
            "id": 7,
            "model": base_model(),
            "context": text_context,
            "options": {"apiKey": "test", "response": {"status": 401, "headers": json!({"content-type": "application/json"}), "body": json!({"message": "unauthorized"}).to_string()}}
        }),
        json!({
            "id": 8,
            "model": base_model(),
            "context": text_context,
            "options": {"apiKey": "test", "response": {"status": 502, "headers": json!({"content-type": "application/json"}), "body": json!({"error": "string error"}).to_string()}}
        }),
        json!({
            "id": 9,
            "model": base_model(),
            "context": text_context,
            "options": {"apiKey": "test", "response": {"status": 503, "headers": json!({"content-type": "application/json"}), "body": json!({"error": {"message": "nested", "type": "x"}}).to_string()}}
        }),
    ];
    for (id, headers, body) in [
        (10, json!({"content-type":"text/plain"}), "gateway returned text"),
        (11, json!({"content-type":"application/json"}), "not json"),
        (12, json!({"content-type":"application/json"}), "{\"choices\":[]}"),
    ] {
        let mut request = cases[0].clone();
        request["id"] = json!(id);
        request["options"]["response"] = json!({"status":200,"headers":headers,"body":body});
        cases.push(request);
    }
    let mut empty_key = cases[0].clone();
    empty_key["id"] = json!(13);
    empty_key["options"]["apiKey"] = json!("");
    cases.push(empty_key);
    for (id, field, value) in [
        (15, "payloadError", json!("payload rejected")),
        (16, "responseError", json!({"message":"raw response rejected"})),
        (
            17,
            "replacePayload",
            json!({"model":"override","messages":[],"stream":false}),
        ),
    ] {
        let mut request = cases[0].clone();
        request["id"] = json!(id);
        request["options"][field] = value;
        cases.push(request);
    }
    let mut retry = cases[0].clone();
    retry["id"] = json!(18);
    retry["options"]["maxRetries"] = json!(1);
    retry["options"]["response"] = json!({"status":500,"headers":{"content-type":"application/json","retry-after-ms":"0"},"body":"{\"error\":{\"message\":\"retry exhausted\"}}"});
    cases.push(retry);
    let mut slow_body = cases[0].clone();
    slow_body["id"] = json!(19);
    slow_body["options"]["timeoutMs"] = json!(10);
    slow_body["options"]["response"]["bodyDelayMs"] = json!(100);
    cases.push(slow_body);
    cases
}

#[test]
fn openrouter_images_matches_the_pinned_typescript_module() {
    let mut ts = oracle::TsOracle::spawn(
        "openrouter-images-oracle.ts",
        "packages/ai/src/api/openrouter-images.ts",
    );
    for request in fixtures() {
        assert_eq!(observed(&request), ts.request(&request), "request {request}");
    }
}

#[test]
fn image_response_text_keeps_native_utf16_against_frozen_sdk() {
    use pi_js_compat::{js_json, js_json::JsValue};
    let body = r#"{"id":"img-native","choices":[{"message":{"content":"image caption \ud800", "images":[]}}]}"#;
    let fetch = Arc::new(FixtureFetch {
        response: FixtureResponse {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.into(),
            body_delay_ms: None,
        },
        requests: Mutex::new(Vec::new()),
    });
    let replacement =
        pi_js_compat::js_json::parse_lossless(r#"{"model":"override","caption":"native \ud801","stream":false}"#)
            .unwrap();
    let payload = replacement.clone();
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some("test".into()),
            fetch: Some(fetch.clone()),
            on_payload: Some(Arc::new(move |_, _| {
                let payload = payload.clone();
                Box::pin(async move { Ok(Some(payload)) })
            })),
            ..Default::default()
        },
        ..Default::default()
    };
    let context = json!({"input":[{"type":"text","text":"draw"}]});
    let output = block_on(generate_images(
        model_from_json(&base_model()),
        context_from_json(&context),
        options,
    ))
    .unwrap();
    let mut actual = js_json!(output);
    actual["timestamp"] = js_json!(0);
    let mut source = oracle::TsOracle::spawn(
        "openrouter-images-oracle.ts",
        "packages/ai/src/api/openrouter-images.ts",
    );
    let mut request: JsValue = json!({"id":14,"model":base_model(),"context":context,"options":{"apiKey":"test","response":{"status":200,"headers":{"content-type":"application/json"},"body":body}}}).into();
    request["options"]["replacePayload"] = replacement;
    let expected = source.request(&request);
    assert_eq!(actual, expected["output"]);
    let requests = fetch.requests.lock().unwrap();
    let sent = String::from_utf8(requests[0].body.clone().unwrap()).unwrap();
    assert_eq!(js_json!(sent), expected["request"]["body"]);
}
