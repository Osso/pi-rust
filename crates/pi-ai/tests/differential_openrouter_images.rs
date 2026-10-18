#[path = "support/oracle.rs"]
mod oracle;
use indexmap::IndexMap;
use pi_ai::api::openrouter_images::generate_images;
use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ImagesOptions, ImagesRequestOptions,
};
use pi_ai::types::{ImagesContext, ImagesModel};
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

fn model_from_json(value: &Value) -> ImagesModel {
    serde_json::from_value(value.clone()).expect("fixture model deserializes")
}

fn context_from_json(value: &Value) -> ImagesContext {
    serde_json::from_value(value.clone()).expect("fixture context deserializes")
}

fn headers_from_json(value: &Value) -> Option<IndexMap<String, Option<String>>> {
    value.as_object().map(|object| {
        object
            .iter()
            .map(|(name, value)| (name.clone(), value.as_str().map(str::to_owned)))
            .collect()
    })
}

fn normalize_headers(headers: &[(String, String)]) -> Value {
    let mut record = Map::new();
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        if name == "user-agent" || name.starts_with("x-stainless-") {
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
    };
    let fetch = Arc::new(FixtureFetch {
        response: fixture,
        requests: Mutex::new(Vec::new()),
    });
    let signal = CancellationToken::new();
    if option_values.get("aborted").and_then(Value::as_bool).unwrap_or(false) {
        signal.cancel();
    }
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: option_values.get("apiKey").and_then(Value::as_str).map(str::to_owned),
            headers: option_values.get("headers").and_then(headers_from_json),
            fetch: Some(fetch.clone()),
            signal: Some(signal),
            ..Default::default()
        },
        ..Default::default()
    };

    let output = block_on(generate_images(model, context, options)).expect("generation returns a result");
    let requests = fetch.requests.lock().expect("requests lock").clone();
    let recorded = match requests.first() {
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
}

struct FixtureFetch {
    response: FixtureResponse,
    requests: Mutex<Vec<HttpRequest>>,
}

impl Fetch for FixtureFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().expect("requests lock").push(request);
        let response = HttpResponse {
            status: self.response.status,
            status_text: String::new(),
            headers: self.response.headers.clone(),
            body: Box::new(BytesBody {
                chunk: Some(self.response.body.as_bytes().to_vec()),
            }),
        };
        Box::pin(async move { Ok(response) })
    }
}

struct BytesBody {
    chunk: Option<Vec<u8>>,
}

impl HttpBody for BytesBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.chunk.take();
        Box::pin(async move { chunk.map(Ok) })
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
    vec![
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
    ]
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
