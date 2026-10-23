use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_ai::api::cloudflare_gateway_binding::{
    AiGatewayBinding, AiGatewayBindingGateway, AiGatewayUniversalRequest, CLOUDFLARE_GATEWAY_BINDING_AUTH_SENTINEL,
    GatewayBindingFetchOptions, create_gateway_binding_fetch,
};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::abort::AbortSignal;

const BASE_URL: &str = "https://gateway.ai.cloudflare.com/v1/account-id/my-gateway";

type Runs = Arc<Mutex<Vec<(String, AiGatewayUniversalRequest, bool)>>>;

#[derive(Clone)]
struct FakeBinding {
    runs: Runs,
    response: FakeResponse,
}

#[derive(Clone)]
struct FakeResponse {
    status: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Default for FakeResponse {
    fn default() -> Self {
        Self {
            status: 200,
            status_text: "OK".to_owned(),
            headers: Vec::new(),
            body: b"{}".to_vec(),
        }
    }
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
        self.binding
            .runs
            .lock()
            .unwrap()
            .push((self.id.clone(), data, signal.is_some()));
        let response = self.binding.response.clone();
        Box::pin(async move {
            Ok(HttpResponse {
                status: response.status,
                status_text: response.status_text,
                headers: response.headers,
                body: Box::new(FixedBody {
                    chunks: VecDeque::from([response.body]),
                }),
            })
        })
    }
}

struct FixedBody {
    chunks: VecDeque<Vec<u8>>,
}

trait ExpectFetchError {
    fn expect_fetch_error(self, message: &str) -> CallbackError;
}

impl ExpectFetchError for Result<HttpResponse, CallbackError> {
    fn expect_fetch_error(self, message: &str) -> CallbackError {
        match self {
            Ok(_) => panic!("{message}"),
            Err(error) => error,
        }
    }
}

impl HttpBody for FixedBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.chunks.pop_front();
        Box::pin(async move { chunk.map(Ok) })
    }
}

fn binding(response: FakeResponse) -> (FakeBinding, Runs) {
    let runs = Arc::new(Mutex::new(Vec::new()));
    (
        FakeBinding {
            runs: Arc::clone(&runs),
            response,
        },
        runs,
    )
}

fn fetch_for(binding: FakeBinding) -> impl Fetch {
    create_gateway_binding_fetch(GatewayBindingFetchOptions {
        binding: Arc::new(binding),
        base_url: BASE_URL.to_owned(),
        gateway: "my-gateway".to_owned(),
    })
    .expect("gateway fetch")
}

fn request(url: &str, method: &str, headers: Vec<(String, String)>, body: Option<&str>) -> HttpRequest {
    HttpRequest {
        method: method.to_owned(),
        url: url.to_owned(),
        headers,
        body: body.map(|body| body.as_bytes().to_vec()),
    }
}

async fn read_text(mut body: Box<dyn HttpBody>) -> String {
    let mut text = String::new();
    while let Some(chunk) = body.next_chunk().await {
        text.push_str(&String::from_utf8_lossy(&chunk.expect("chunk")));
    }
    text
}

#[tokio::test]
async fn derives_provider_and_endpoint_from_gateway_passthrough_urls() {
    let (binding, runs) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);

    fetch
        .fetch(
            request(
                &format!("{BASE_URL}/anthropic/v1/messages"),
                "POST",
                Vec::new(),
                Some(r#"{"model":"claude"}"#),
            ),
            None,
        )
        .await
        .expect("first call");
    fetch
        .fetch(
            request(
                &format!("{BASE_URL}/openai/responses"),
                "POST",
                Vec::new(),
                Some(r#"{"model":"gpt"}"#),
            ),
            None,
        )
        .await
        .expect("second call");
    fetch
        .fetch(
            request(
                &format!("{BASE_URL}/workers-ai/v1/chat/completions"),
                "POST",
                Vec::new(),
                Some(r#"{"model":"@cf/meta/llama"}"#),
            ),
            None,
        )
        .await
        .expect("third call");

    let runs = runs.lock().unwrap();
    assert_eq!(
        runs.iter()
            .map(|(_, data, _)| (data.provider.as_str(), data.endpoint.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("anthropic", "v1/messages"),
            ("openai", "responses"),
            ("workers-ai", "v1/chat/completions"),
        ]
    );
    assert!(runs.iter().all(|(gateway, _, _)| gateway == "my-gateway"));
    assert_eq!(runs[0].1.query, serde_json::json!({"model": "claude"}));
}

#[tokio::test]
async fn keeps_the_query_string_in_the_endpoint() {
    let (binding, runs) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);
    fetch
        .fetch(
            request(
                &format!("{BASE_URL}/openai/responses?beta=true"),
                "POST",
                Vec::new(),
                Some("{}"),
            ),
            None,
        )
        .await
        .expect("call");
    assert_eq!(runs.lock().unwrap()[0].1.endpoint, "responses?beta=true");
}

#[tokio::test]
async fn lowercases_header_names_and_collapses_case_variants() {
    let (binding, runs) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);
    fetch
        .fetch(
            request(
                &format!("{BASE_URL}/anthropic/v1/messages"),
                "POST",
                vec![
                    ("Anthropic-Version".to_owned(), "2023-06-01".to_owned()),
                    ("X-Api-Key".to_owned(), "first".to_owned()),
                    ("x-api-key".to_owned(), "second".to_owned()),
                ],
                Some("{}"),
            ),
            None,
        )
        .await
        .expect("call");
    let headers = &runs.lock().unwrap()[0].1.headers;
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    assert_eq!(headers["x-api-key"], "second");
}

#[tokio::test]
async fn strips_gateway_auth_and_derived_headers_and_forwards_the_rest() {
    let (binding, runs) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);
    fetch
        .fetch(
            request(
                &format!("{BASE_URL}/anthropic/v1/messages"),
                "POST",
                vec![
                    ("Content-Type".to_owned(), "application/json".to_owned()),
                    ("Content-Length".to_owned(), "17".to_owned()),
                    (
                        "CF-AIG-Authorization".to_owned(),
                        format!("Bearer {CLOUDFLARE_GATEWAY_BINDING_AUTH_SENTINEL}"),
                    ),
                    ("cf-aig-metadata".to_owned(), "{\"user\":\"42\"}".to_owned()),
                    ("anthropic-version".to_owned(), "2023-06-01".to_owned()),
                    ("x-api-key".to_owned(), "provider-key".to_owned()),
                ],
                Some("{}"),
            ),
            None,
        )
        .await
        .expect("call");
    let headers = &runs.lock().unwrap()[0].1.headers;
    assert!(!headers.contains_key("cf-aig-authorization"));
    assert!(!headers.contains_key("content-length"));
    assert!(!headers.contains_key("host"));
    assert_eq!(headers["cf-aig-metadata"], "{\"user\":\"42\"}");
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    assert_eq!(headers["x-api-key"], "provider-key");
}

#[tokio::test]
async fn forwards_the_abort_signal() {
    let (binding, runs) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    fetch
        .fetch(
            request(
                &format!("{BASE_URL}/anthropic/v1/messages"),
                "POST",
                Vec::new(),
                Some("{}"),
            ),
            Some(signal),
        )
        .await
        .expect("call");
    assert!(runs.lock().unwrap()[0].2);
}

#[tokio::test]
async fn returns_the_binding_response_untouched() {
    let (binding, _) = binding(FakeResponse {
        status: 200,
        status_text: "OK".to_owned(),
        headers: vec![
            ("content-type".to_owned(), "text/event-stream".to_owned()),
            ("cf-aig-log-id".to_owned(), "log-1".to_owned()),
        ],
        body: b"data: {}\n\n".to_vec(),
    });
    let fetch = fetch_for(binding);
    let response = fetch
        .fetch(
            request(
                &format!("{BASE_URL}/workers-ai/v1/chat/completions"),
                "POST",
                Vec::new(),
                Some("{}"),
            ),
            None,
        )
        .await
        .expect("call");
    assert_eq!(response.status, 200);
    assert_eq!(
        response
            .headers
            .iter()
            .find(|(name, _)| name == "cf-aig-log-id")
            .map(|(_, value)| value.as_str()),
        Some("log-1")
    );
    let text = read_text(response.body).await;
    assert_eq!(text, "data: {}\n\n");
}

#[tokio::test]
async fn rejects_in_prefix_requests_the_universal_endpoint_cannot_express() {
    let (binding, runs) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);

    let error = fetch
        .fetch(
            request(&format!("{BASE_URL}/anthropic/v1/messages"), "GET", Vec::new(), None),
            None,
        )
        .await
        .expect_fetch_error("GET must reject");
    assert!(error.message.contains("cannot express GET"));

    let error = fetch
        .fetch(
            request(
                &format!("{BASE_URL}/anthropic/v1/messages"),
                "POST",
                Vec::new(),
                Some("not json"),
            ),
            None,
        )
        .await
        .expect_fetch_error("non-JSON must reject");
    assert!(error.message.contains("non-JSON body"));

    let error = fetch
        .fetch(
            request(&format!("{BASE_URL}/anthropic"), "POST", Vec::new(), Some("{}")),
            None,
        )
        .await
        .expect_fetch_error("missing endpoint must reject");
    assert!(error.message.contains("missing provider/endpoint path"));

    let error = fetch
        .fetch(
            request(&format!("{BASE_URL}/anthropic/v1/messages"), "POST", Vec::new(), None),
            None,
        )
        .await
        .expect_fetch_error("missing body must reject");
    assert!(error.message.contains("missing body"));
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rejects_urls_outside_the_gateway_prefix() {
    let (binding, runs) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);

    let error = fetch
        .fetch(
            request(
                "https://api.openai.com/v1/chat/completions",
                "POST",
                Vec::new(),
                Some("{}"),
            ),
            None,
        )
        .await
        .expect_fetch_error("other origin must reject");
    assert!(error.message.contains("outside the configured gateway prefix"));

    let error = fetch
        .fetch(
            request(
                "https://gateway.ai.cloudflare.com/v1/other-account/my-gateway/anthropic/v1/messages",
                "POST",
                Vec::new(),
                Some("{}"),
            ),
            None,
        )
        .await
        .expect_fetch_error("other gateway path must reject");
    assert!(error.message.contains("outside the configured gateway prefix"));
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn matches_and_splits_on_the_url_normalized_path() {
    let (binding, runs) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);

    fetch
        .fetch(
            request(
                &format!("{BASE_URL}/anthropic/../anthropic/v1/./messages"),
                "POST",
                Vec::new(),
                Some(r#"{"model":"claude"}"#),
            ),
            None,
        )
        .await
        .expect("normalized call");
    {
        let runs = runs.lock().unwrap();
        assert_eq!(
            (runs[0].1.provider.as_str(), runs[0].1.endpoint.as_str()),
            ("anthropic", "v1/messages")
        );
    }

    let error = fetch
        .fetch(
            request(
                &format!("{BASE_URL}/../other-gateway/anthropic/v1/messages"),
                "POST",
                Vec::new(),
                Some("{}"),
            ),
            None,
        )
        .await
        .expect_fetch_error("escaping dot segments must reject");
    assert!(error.message.contains("outside the configured gateway prefix"));
}

#[tokio::test]
async fn rejects_unparseable_urls() {
    let (binding, _) = binding(FakeResponse::default());
    let fetch = fetch_for(binding);
    let error = fetch
        .fetch(request("not a url", "POST", Vec::new(), Some("{}")), None)
        .await
        .expect_fetch_error("invalid URL must reject");
    assert!(error.message.contains("outside the configured gateway prefix"));
}
