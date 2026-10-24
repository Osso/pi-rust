use std::sync::{Arc, Mutex};

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse, StreamOptions};
use pi_ai::types::{Context, Model, StopReason};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::JsString;

#[derive(Default)]
struct Capture(Mutex<Vec<HttpRequest>>);
impl Fetch for Capture {
    fn fetch(
        &self,
        request: HttpRequest,
        _: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.0.lock().unwrap().push(request);
        Box::pin(async { Err(CallbackError::new("capture complete")) })
    }
}

fn model(api: &str, provider: &str) -> Model {
    serde_json::from_value(serde_json::json!({
        "id":"probe", "name":"probe", "api":api, "provider":provider,
        "baseUrl":"https://provider.invalid/v1", "reasoning":false, "input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},
        "contextWindow":10000,"maxTokens":1000
    }))
    .unwrap()
}

fn stream(model: Model, options: StreamOptions) -> pi_ai::utils::event_stream::AssistantMessageEventStream {
    let context = Context {
        system_prompt: None,
        messages: Vec::new(),
        tools: None,
    };
    match model.api.as_str() {
        "openai-completions" => pi_ai::api::openai_completions::stream(model, context, options),
        "openai-responses" => pi_ai::api::openai_responses::stream(model, context, options),
        "azure-openai-responses" => pi_ai::api::azure_openai_responses::stream(model, context, options),
        "mistral-conversations" => pi_ai::api::mistral_conversations::stream(
            model,
            context,
            pi_ai::api::mistral_conversations::MistralOptions::from_stream_options(options),
        ),
        _ => unreachable!(),
    }
}

const APIS: &[(&str, &str)] = &[
    ("openai-completions", "openai"),
    ("openai-responses", "openai"),
    ("azure-openai-responses", "azure-openai-responses"),
    ("mistral-conversations", "mistral"),
];

#[tokio::test]
async fn unmatched_auth_header_is_rejected_before_fetch() {
    for (api, provider) in APIS {
        let fetch = Arc::new(Capture::default());
        let mut options = StreamOptions::default();
        options.request.fetch = Some(fetch.clone());
        options.request.api_key = Some(JsString::from_utf16(vec![0xd800]));
        options.request.max_retries = Some(0.0);
        let mut stream = stream(model(api, provider), options);
        while stream.next().await.is_some() {}
        let output = stream.result().await.unwrap();
        assert_eq!(output.stop_reason, StopReason::Error, "{api}");
        let error = output.error_message.unwrap();
        assert!(error.contains("ByteString"), "{api}: {error:?}");
        assert!(error.contains("55296"), "{api}: {error:?}");
        assert!(fetch.0.lock().unwrap().is_empty(), "{api}");
    }
}

#[tokio::test]
async fn unmatched_endpoint_units_convert_only_at_request_url_boundary() {
    for (api, provider) in APIS {
        let fetch = Arc::new(Capture::default());
        let mut options = StreamOptions::default();
        options.request.fetch = Some(fetch.clone());
        options.request.api_key = Some("key".into());
        options.request.max_retries = Some(0.0);
        let mut model = model(api, provider);
        model.base_url = JsString::from("https://provider.invalid/") + JsString::from_utf16(vec![0xd800]);
        let mut stream = stream(model, options);
        while stream.next().await.is_some() {}
        let requests = fetch.0.lock().unwrap();
        assert_eq!(requests.len(), 1, "{api}");
        assert!(requests[0].url.contains("%EF%BF%BD"), "{api}: {}", requests[0].url);
    }
}

struct Answers(Mutex<std::collections::VecDeque<JsString>>);
impl pi_ai::auth::AuthInteraction for Answers {
    fn signal(&self) -> AbortSignal {
        AbortSignal::default()
    }
    fn prompt(&self, _: pi_ai::auth::AuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
        let answer = self.0.lock().unwrap().pop_front().expect("answer");
        Box::pin(async move { Ok(answer) })
    }
    fn notify(&self, _: pi_ai::auth::AuthEvent) {}
}

#[tokio::test]
async fn vertex_login_retains_raw_project_location_and_credential_path() {
    let project = JsString::from_utf16(vec![0x70, 0xd800]);
    let location = JsString::from_utf16(vec![0x6c, 0xdc00]);
    let path = JsString::from_utf16(vec![0x2f, 0xd801]);
    let interaction: Arc<dyn pi_ai::auth::AuthInteraction> = Arc::new(Answers(Mutex::new(
        [
            "service-account".into(),
            project.clone(),
            location.clone(),
            path.clone(),
        ]
        .into_iter()
        .collect(),
    )));
    let provider = pi_ai::providers::google_vertex::google_vertex_provider();
    let credential = provider.auth().api_key.as_ref().unwrap().login().unwrap()(interaction)
        .await
        .unwrap();
    let env = credential.env.unwrap();
    assert_eq!(env.get("GOOGLE_CLOUD_PROJECT"), Some(&project));
    assert_eq!(env.get("GOOGLE_CLOUD_LOCATION"), Some(&location));
    assert_eq!(env.get("GOOGLE_APPLICATION_CREDENTIALS"), Some(&path));
}

#[test]
fn stored_radius_config_keeps_endpoint_and_rejects_invalid_scalar_model_id() {
    use pi_ai::providers::radius_config::{get_radius_credential_config, sanitize_radius_gateway_config};
    use pi_js_compat::js_json::{JsObject, parse_lossless};
    let config = parse_lossless(r#"{"baseUrl":"https://example.com/\ud800","models":["shallow-invalid"]}"#).unwrap();
    let credential = pi_ai::auth::OAuthCredential {
        refresh: "refresh".into(),
        access: "access".into(),
        expires: 1.0,
        extra: JsObject::from_iter([("gatewayConfig".into(), config)]),
    };
    let result = get_radius_credential_config(Some(&credential)).unwrap().unwrap();
    assert!(result.base_url.units().contains(&0xd800));
    assert!(result.models.is_empty());
    let invalid = parse_lossless(r#"{"baseUrl":"https://example.com","models":[{"id":"\ud800","name":"model","reasoning":false,"input":[],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10,"maxTokens":2}]}"#).unwrap();
    let error = sanitize_radius_gateway_config(&invalid).unwrap_err();
    assert!(error.message.starts_with("Invalid Radius gateway model: "));
}
