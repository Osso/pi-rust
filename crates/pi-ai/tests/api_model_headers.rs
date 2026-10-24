//! Wire-header proof across API adapters after Models.getAuth stops copying model headers.
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use pi_ai::api::{
    anthropic_messages_lazy, azure_openai_responses, google_generative_ai, google_vertex, mistral_conversations,
    openai_codex_responses, openai_completions, openai_responses,
};
use pi_ai::auth::{ProviderAuth, env_api_key_auth};
use pi_ai::models::{
    CreateModelsOptions, CreateProviderOptions, Models, MutableModels, ProviderApi, create_models, create_provider,
};
use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ImagesOptions, ImagesRequestOptions,
    ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{Context, ImagesContext, ImagesModel, Model, TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use serde_json::json;

const SENTINEL: &str = "x-model-header-proof";
const KEY: &str = "x.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiaGVhZGVyLXByb29mIn19.x";

#[derive(Default)]
struct HeaderFetch(Mutex<Vec<HttpRequest>>);
struct EmptyBody;
impl HttpBody for EmptyBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { None })
    }
}
impl Fetch for HeaderFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(HttpResponse {
                status: 400,
                status_text: String::new(),
                headers: Vec::new(),
                body: Box::new(EmptyBody),
            })
        })
    }
}

fn assert_wire_headers(fetch: &HeaderFetch, api: &str, expects_model_header: bool) {
    let requests = fetch.0.lock().unwrap();
    assert_eq!(requests.len(), 1, "{api} must reach fake HTTP exactly once");
    let headers = &requests[0].headers;
    let model_header = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(SENTINEL))
        .map(|(_, value)| value.as_str());
    assert_eq!(model_header, expects_model_header.then_some("model-value"), "{api}");
    assert!(
        headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("x-request-header-proof") && value == "request-value"),
        "{api} request header missing"
    );
}

fn proof_model(api: &str) -> Model {
    serde_json::from_value(json!({
        "id":"model", "name":"Header proof", "api":api, "provider":"header-proof",
        "baseUrl":"https://headers.test/v1", "reasoning":false, "input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},
        "contextWindow":4096, "maxTokens":128, "headers":{SENTINEL:"model-value"}
    }))
    .unwrap()
}

fn proof_options(fetch: Arc<HeaderFetch>) -> SimpleStreamOptions {
    SimpleStreamOptions {
        stream: StreamOptions {
            // Keep the Codex HTTP seam selected; Auto selects a real WebSocket.
            transport: Some(pi_ai::types::Transport::Sse),
            request: ProviderRequestOptions {
                api_key: Some(KEY.into()),
                fetch: Some(fetch),
                max_retries: Some(0.0),
                headers: Some(IndexMap::from([(
                    "x-request-header-proof".into(),
                    Some("request-value".into()),
                )])),
                env: Some(IndexMap::from([
                    ("AWS_REGION".into(), "us-east-1".into()),
                    ("AWS_BEARER_TOKEN_BEDROCK".into(), "test-key".into()),
                    ("GOOGLE_CLOUD_PROJECT".into(), "project".into()),
                    ("GOOGLE_CLOUD_LOCATION".into(), "us-central1".into()),
                    ("AZURE_OPENAI_BASE_URL".into(), "https://headers.test/openai".into()),
                ])),
                ..Default::default()
            },
            extra: [
                ("project".into(), json!("project")),
                ("location".into(), json!("us-central1")),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn every_api_adapter_keeps_its_frozen_model_header_behavior_on_the_wire() {
    let apis: Vec<(&str, Arc<dyn ProviderStreams>)> = vec![
        (
            "anthropic-messages",
            Arc::new(anthropic_messages_lazy::anthropic_messages_api()),
        ),
        (
            "openai-completions",
            Arc::new(openai_completions::open_ai_completions_api()),
        ),
        ("openai-responses", Arc::new(openai_responses::open_ai_responses_api())),
        (
            "openai-codex-responses",
            Arc::new(openai_codex_responses::openai_codex_responses_api()),
        ),
        (
            "azure-openai-responses",
            Arc::new(azure_openai_responses::azure_open_ai_responses_api()),
        ),
        (
            "google-generative-ai",
            Arc::new(google_generative_ai::google_generative_ai_api()),
        ),
        ("google-vertex", Arc::new(google_vertex::google_vertex_api())),
        (
            "mistral-conversations",
            Arc::new(mistral_conversations::mistral_conversations_api()),
        ),
        (
            "bedrock-converse-stream",
            Arc::new(pi_ai::api::bedrock_converse_stream::BedrockProviderModule),
        ),
    ];
    for (api, streams) in apis {
        let model = proof_model(api);
        let models = create_models(CreateModelsOptions::default());
        models.set_provider(create_provider(CreateProviderOptions {
            id: model.provider.clone(),
            name: None,
            base_url: None,
            headers: None,
            auth: ProviderAuth::api_key(Arc::new(env_api_key_auth("Proof key", &[]))),
            models: vec![model.clone()],
            fetch_models: None,
            filter_models: None,
            api: ProviderApi::Single(streams),
        }));
        let fetch = Arc::new(HeaderFetch::default());
        let result = models
            .complete_simple(
                model,
                Context {
                    system_prompt: None,
                    messages: Vec::new(),
                    tools: None,
                },
                proof_options(fetch.clone()),
            )
            .await;
        assert!(result.is_ok(), "{api}: {result:?}");
        assert!(
            !fetch.0.lock().unwrap().is_empty(),
            "{api} failed before HTTP: {result:?}"
        );
        // Frozen Bedrock :214 consumes options.headers only; all other chat APIs consume model.headers.
        assert_wire_headers(&fetch, api, api != "bedrock-converse-stream");
    }
    let fetch = Arc::new(HeaderFetch::default());
    let model: ImagesModel = serde_json::from_value(json!({
        "id":"model", "name":"Header proof", "api":"openrouter-images", "provider":"openrouter",
        "baseUrl":"https://headers.test/v1", "input":["text"], "output":["image"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}, "headers":{SENTINEL:"model-value"}
    }))
    .unwrap();
    pi_ai::api::openrouter_images::generate_images(
        model,
        ImagesContext {
            input: vec![TextOrImageContent::Text(TextContent::new("Draw a tree"))],
        },
        ImagesOptions {
            request: ImagesRequestOptions {
                api_key: Some("key".into()),
                fetch: Some(fetch.clone()),
                headers: Some(IndexMap::from([(
                    "x-request-header-proof".into(),
                    Some("request-value".into()),
                )])),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_wire_headers(&fetch, "openrouter-images", true);
}
