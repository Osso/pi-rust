#[path = "support/json_compare.rs"]
mod json_compare;
#[path = "support/oracle.rs"]
mod oracle;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_ai::api::cloudflare_gateway_binding::{
    AiGatewayBinding, AiGatewayBindingGateway, AiGatewayUniversalRequest, GatewayBindingFetchOptions,
    create_gateway_binding_fetch,
};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};

#[derive(Clone)]
struct FakeBinding {
    runs: Arc<Mutex<Vec<Value>>>,
    status: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

struct FakeGateway {
    binding: FakeBinding,
    id: String,
}

impl AiGatewayBinding for FakeBinding {
    fn gateway(&self, id: &str) -> Arc<dyn AiGatewayBindingGateway> {
        Arc::new(FakeGateway {
            binding: self.clone(),
            id: id.to_owned(),
        })
    }
}

impl AiGatewayBindingGateway for FakeGateway {
    fn run(
        &self,
        data: AiGatewayUniversalRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.binding.runs.lock().unwrap().push(json!({
            "gateway": self.id,
            "provider": data.provider,
            "endpoint": data.endpoint,
            "headers": data.headers,
            "query": data.query,
            "hasSignal": signal.is_some(),
        }));
        let status = self.binding.status;
        let status_text = self.binding.status_text.clone();
        let headers = self.binding.headers.clone();
        let body = self.binding.body.clone();
        Box::pin(async move {
            Ok(HttpResponse {
                status,
                status_text,
                headers,
                body: Box::new(FixedBody {
                    chunks: VecDeque::from([body]),
                }),
            })
        })
    }
}

struct FixedBody {
    chunks: VecDeque<Vec<u8>>,
}

impl HttpBody for FixedBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.chunks.pop_front();
        Box::pin(async move { chunk.map(Ok) })
    }
}

/// Rebuilds a value with number normalization only; key order is untouched.
fn canonical(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        Value::Object(object) => {
            Value::Object(object.into_iter().map(|(key, value)| (key, canonical(value))).collect())
        }
        Value::Number(number) => match number.as_f64() {
            Some(float) => pi_js_compat::number::js_number_value(float),
            None => Value::Number(number),
        },
        other => other,
    }
}

async fn read_text(mut body: Box<dyn HttpBody>) -> String {
    let mut text = String::new();
    while let Some(chunk) = body.next_chunk().await {
        text.push_str(&String::from_utf8_lossy(&chunk.expect("chunk")));
    }
    text
}

async fn observe(scenario: &Value) -> Value {
    let response = &scenario["response"];
    let mut headers = vec![("content-type".to_owned(), "text/plain".to_owned())];
    if let Some(extra) = response["headers"].as_object() {
        headers.extend(
            extra
                .iter()
                .filter_map(|(name, value)| value.as_str().map(|value| (name.clone(), value.to_owned()))),
        );
    }
    let runs = Arc::new(Mutex::new(Vec::new()));
    let binding = FakeBinding {
        runs: Arc::clone(&runs),
        status: response["status"].as_u64().unwrap_or(200) as u16,
        status_text: response["statusText"].as_str().unwrap_or("OK").to_owned(),
        headers,
        body: response["body"].as_str().unwrap_or("{}").as_bytes().to_vec(),
    };
    let fetch = create_gateway_binding_fetch(GatewayBindingFetchOptions {
        binding: Arc::new(binding),
        base_url: scenario["baseUrl"].as_str().expect("baseUrl").to_owned(),
        gateway: scenario["gateway"].as_str().expect("gateway").to_owned(),
    })
    .expect("gateway fetch");

    let mut calls = Vec::new();
    for call in scenario["calls"].as_array().expect("calls") {
        let headers = call["headers"]
            .as_object()
            .map(|headers| {
                headers
                    .iter()
                    .filter_map(|(name, value)| value.as_str().map(|value| (name.clone(), value.to_owned())))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let run_count = runs.lock().unwrap().len();
        let signal = call["signal"].as_bool().unwrap_or(false).then(AbortSignal::default);
        let request = HttpRequest {
            method: call["method"].as_str().unwrap_or("GET").to_owned(),
            url: call["url"].as_str().expect("url").to_owned(),
            headers,
            body: call["body"].as_str().map(|body| body.as_bytes().to_vec()),
        };
        match fetch.fetch(request, signal).await {
            Ok(response) => {
                let run = {
                    let runs = runs.lock().unwrap();
                    if runs.len() > run_count {
                        runs.last().cloned()
                    } else {
                        None
                    }
                };
                calls.push(json!({
                    "ok": true,
                    "run": run,
                    "response": {
                        "status": response.status,
                        "statusText": response.status_text,
                        "headers": response.headers.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
                        "body": read_text(response.body).await,
                    },
                }));
            }
            Err(error) => calls.push(json!({"ok": false, "error": error.message})),
        }
    }

    canonical(json!({"id": scenario["id"], "calls": calls}))
}

fn scenario(id: usize, calls: Value, response: Value) -> Value {
    json!({
        "id": id,
        "baseUrl": "https://gateway.ai.cloudflare.com/v1/account-id/my-gateway",
        "gateway": "my-gateway",
        "calls": calls,
        "response": response,
    })
}

fn scenarios() -> Vec<Value> {
    let base = "https://gateway.ai.cloudflare.com/v1/account-id/my-gateway";
    let mut scenarios = Vec::new();

    scenarios.push(scenario(
        0,
        json!([
            {"url": format!("{base}/anthropic/v1/messages"), "method": "POST", "body": "{\"model\":\"claude\"}"},
            {"url": format!("{base}/openai/responses"), "method": "POST", "body": "{\"model\":\"gpt\"}"},
            {"url": format!("{base}/workers-ai/v1/chat/completions"), "method": "POST", "body": "{\"model\":\"@cf/meta/llama\"}"},
        ]),
        json!({"status": 200, "statusText": "OK", "body": "{}"}),
    ));

    scenarios.push(scenario(
        1,
        json!([{"url": format!("{base}/openai/responses?beta=true"), "method": "POST", "body": "{}"}]),
        json!({}),
    ));

    scenarios.push(scenario(
        2,
        json!([{
            "url": format!("{base}/anthropic/v1/messages"),
            "method": "POST",
            "headers": {
                "Content-Type": "application/json",
                "Content-Length": "17",
                "CF-AIG-Authorization": "Bearer cloudflare-gateway-binding",
                "cf-aig-metadata": "{\"user\":\"42\"}",
                "anthropic-version": "2023-06-01",
                "x-api-key": "provider-key",
            },
            "body": "{}",
        }]),
        json!({}),
    ));

    scenarios.push(scenario(
        3,
        json!([
            {"url": format!("{base}/anthropic/../anthropic/v1/./messages"), "method": "POST", "body": "{\"model\":\"claude\"}"},
            {"url": format!("{base}/../other-gateway/anthropic/v1/messages"), "method": "POST", "body": "{}"},
        ]),
        json!({}),
    ));

    scenarios.push(scenario(
        4,
        json!([
            {"url": "https://api.openai.com/v1/chat/completions", "method": "POST", "body": "{}"},
            {"url": "https://gateway.ai.cloudflare.com/v1/other-account/my-gateway/anthropic/v1/messages", "method": "POST", "body": "{}"},
            {"url": "not a url", "method": "POST", "body": "{}"},
        ]),
        json!({}),
    ));

    scenarios.push(scenario(
        5,
        json!([
            {"url": format!("{base}/anthropic/v1/messages"), "method": "GET"},
            {"url": format!("{base}/anthropic/v1/messages"), "method": "POST", "body": "not json"},
            {"url": format!("{base}/anthropic"), "method": "POST", "body": "{}"},
            {"url": format!("{base}/anthropic/v1/messages"), "method": "POST"},
        ]),
        json!({}),
    ));

    scenarios.push(scenario(
        6,
        json!([{"url": format!("{base}/anthropic/v1/messages"), "method": "POST", "body": "{}", "signal": true}]),
        json!({}),
    ));

    scenarios.push(scenario(
        7,
        json!([{
            "url": format!("{base}/workers-ai/v1/chat/completions"),
            "method": "POST",
            "body": "{}",
        }]),
        json!({
            "status": 201,
            "statusText": "Created",
            "headers": {"cf-aig-log-id": "log-1"},
            "body": "data: {}\n\n",
        }),
    ));

    scenarios.push(scenario(
        8,
        json!([{
            "url": format!("{base}/anthropic/v1/messages"),
            "method": "POST",
            "headers": {"Anthropic-Version": "2023-06-01", "X-Api-Key": "first", "x-api-key": "second"},
            "body": "{}",
        }]),
        json!({}),
    ));

    scenarios.push(scenario(
        9,
        json!([{"url": format!("{base}/openai/responses#fragment"), "method": "POST", "body": "{}"}]),
        json!({}),
    ));

    let mut trailing = scenario(
        10,
        json!([{"url": format!("{base}/openai/responses"), "method": "POST", "body": "{}"}]),
        json!({}),
    );
    trailing["baseUrl"] = json!(format!("{base}/"));
    scenarios.push(trailing);

    scenarios
}

#[test]
fn gateway_binding_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn(
        "cloudflare-gateway-binding-oracle.ts",
        "packages/ai/src/api/cloudflare-gateway-binding.ts",
    );
    for scenario in scenarios() {
        let expected = canonical(oracle.request(&scenario));
        let observed = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(observe(&scenario));
        json_compare::assert_json_eq_ordered(&observed, &expected, &format!("scenario {}", scenario["id"]));
    }
}
