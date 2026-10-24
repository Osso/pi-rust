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
