use crate::shared_recording_fetch as recording_fetch;

use std::sync::{Arc, Mutex};

use pi_ai::api::openrouter_images::generate_images;
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse, ImagesOptions, ImagesRequestOptions};
use pi_ai::types::{
    ImagesContext, ImagesModel, ImagesStopReason, Modality, ProviderResponse, TextOrImageContent, UsageCost,
};
use pi_js_compat::abort::AbortSignal;
use recording_fetch::{RecordingFetch, ScriptedResponse};
use serde_json::{Value, json};

fn model() -> ImagesModel {
    ImagesModel {
        properties: Default::default(),
        id: "google/gemini-3.1-flash-image-preview".to_owned(),
        name: "Gemini 3.1 Flash Image Preview".into(),
        api: "openrouter-images".to_owned(),
        provider: "openrouter".to_owned(),
        base_url: ("https://openrouter.ai/api/v1".to_owned()).into(),
        thinking_level_map: None,
        input: vec![Modality::Text, Modality::Image],
        output: vec![Modality::Text, Modality::Image],
        cost: pi_ai::types::ModelCost {
            properties: Default::default(),
            rates: pi_ai::types::ModelCostRates {
                input: 0.015,
                output: 0.03,
                cache_read: 0.003,
                cache_write: 0.01,
            },
        },
        auto_compaction_threshold: None,
        headers: Some(indexmap::IndexMap::from([(
            "HTTP-Referer".into(),
            "https://example.com".into(),
        )])),
    }
}

fn text_context() -> ImagesContext {
    ImagesContext {
        input: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(
            "Generate a dog",
        ))],
    }
}

fn options(api_key: &str) -> ImagesOptions {
    ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some((api_key.to_owned()).into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn options_with_fetch(api_key: &str, fetch: Arc<dyn Fetch>) -> ImagesOptions {
    ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some((api_key.to_owned()).into()),
            fetch: Some(fetch),
            ..Default::default()
        },
        ..Default::default()
    }
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

fn header(request: &HttpRequest, name: &str) -> Option<String> {
    request
        .headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

#[tokio::test]
async fn returns_text_and_images_and_builds_the_sdk_request() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &success_body())]);
    let mut request_options = options("test").request;
    request_options.fetch = Some(fetch.clone());
    request_options.headers = Some(indexmap::IndexMap::from([("X-Extra".into(), Some("1".into()))]));
    let options = ImagesOptions {
        request: request_options,
        ..Default::default()
    };

    let output = generate_images(model(), text_context(), options)
        .await
        .expect("generation returns a result");
    assert_eq!(output.stop_reason, ImagesStopReason::Stop);
    assert_eq!(output.response_id.as_deref(), Some("img-1"));
    assert_eq!(output.output.len(), 3);
    match &output.output[0] {
        TextOrImageContent::Text(text) => assert_eq!(text.text, "Here is your image."),
        other => panic!("expected text, got {other:?}"),
    }
    match &output.output[1] {
        TextOrImageContent::Image(image) => {
            assert_eq!(image.mime_type, "image/png");
            assert_eq!(image.data, "ZmFrZS1wbmc=");
        }
        other => panic!("expected image, got {other:?}"),
    }
    match &output.output[2] {
        TextOrImageContent::Image(image) => {
            assert_eq!(image.mime_type, "image/jpeg");
            assert_eq!(image.data, "AAAA");
        }
        other => panic!("expected image, got {other:?}"),
    }

    let usage = output.usage.expect("usage");
    assert_eq!(usage.input, 12.0);
    assert_eq!(usage.output, 34.0);
    assert_eq!(usage.cache_read, 0.0);
    assert_eq!(usage.cache_write, 0.0);
    assert_eq!(usage.total_tokens, 46.0);
    let expected = UsageCost {
        input: (0.015 / 1_000_000.0) * 12.0,
        output: (0.03 / 1_000_000.0) * 34.0,
        cache_read: 0.0,
        cache_write: 0.0,
        total: 0.0,
    };
    assert_eq!(usage.cost.input, expected.input);
    assert_eq!(usage.cost.output, expected.output);
    assert_eq!(
        usage.cost.total,
        expected.input + expected.output + expected.cache_read + expected.cache_write
    );

    let requests = fetch.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "https://openrouter.ai/api/v1/chat/completions");
    assert_eq!(header(request, "authorization").as_deref(), Some("Bearer test"));
    assert_eq!(header(request, "content-type").as_deref(), Some("application/json"));
    assert_eq!(header(request, "accept").as_deref(), Some("application/json"));
    assert_eq!(header(request, "http-referer").as_deref(), Some("https://example.com"));
    assert_eq!(header(request, "x-extra").as_deref(), Some("1"));
    assert_eq!(
        String::from_utf8(request.body.clone().expect("body")).expect("body is UTF-8"),
        json!({
            "model": "google/gemini-3.1-flash-image-preview",
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": "Generate a dog"}]
            }],
            "stream": false,
            "modalities": ["image", "text"]
        })
        .to_string()
    );
}

#[tokio::test]
async fn sends_image_inputs_as_data_urls() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &success_body())]);
    let context = ImagesContext {
        input: vec![
            TextOrImageContent::Text(pi_ai::types::TextContent::new("Make it blue")),
            TextOrImageContent::Image(pi_ai::types::ImageContent::new("aGk=", "image/png")),
        ],
    };
    generate_images(model(), context, options_with_fetch("test", fetch.clone()))
        .await
        .expect("generation returns a result");
    let request = &fetch.requests()[0];
    let body: Value = serde_json::from_slice(request.body.as_deref().expect("body")).expect("JSON body");
    assert_eq!(
        body["messages"][0]["content"][1],
        json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,aGk="}})
    );
}

#[tokio::test]
async fn image_only_models_request_image_modality_only() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &success_body())]);
    let mut model = model();
    model.output = vec![Modality::Image];
    generate_images(model, text_context(), options_with_fetch("test", fetch.clone()))
        .await
        .expect("generation returns a result");
    let body: Value = serde_json::from_slice(fetch.requests()[0].body.as_deref().expect("body")).expect("JSON body");
    assert_eq!(body["modalities"], json!(["image"]));
}

#[tokio::test]
async fn usage_math_matches_the_typescript_cache_write_rule() {
    let body = json!({
        "id": "img-2",
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 10,
            "prompt_tokens_details": {"cached_tokens": 40, "cache_write_tokens": 10}
        },
        "choices": []
    })
    .to_string();
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &body)]);
    let output = generate_images(model(), text_context(), options_with_fetch("test", fetch.clone()))
        .await
        .expect("generation returns a result");
    let usage = output.usage.expect("usage");
    assert_eq!(usage.cache_write, 10.0);
    assert_eq!(usage.cache_read, 30.0);
    assert_eq!(usage.input, 60.0);
    assert_eq!(usage.output, 10.0);
    assert_eq!(usage.total_tokens, 110.0);
    assert_eq!(usage.cost.cache_read, (0.003 / 1_000_000.0) * 30.0);
    assert_eq!(usage.cost.cache_write, (0.01 / 1_000_000.0) * 10.0);
    assert!(output.output.is_empty());
    assert_eq!(fetch.requests().len(), 1);
}

#[tokio::test]
async fn cache_write_larger_than_reported_cached_tokens_clamps_cache_read() {
    let body = json!({
        "usage": {
            "prompt_tokens": 5,
            "completion_tokens": 1,
            "prompt_tokens_details": {"cached_tokens": 2, "cache_write_tokens": 7}
        },
        "choices": []
    })
    .to_string();
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &body)]);
    let output = generate_images(model(), text_context(), options_with_fetch("test", fetch.clone()))
        .await
        .expect("generation returns a result");
    let usage = output.usage.expect("usage");
    assert_eq!(usage.cache_read, 0.0);
    assert_eq!(usage.cache_write, 7.0);
    assert_eq!(usage.input, 0.0);
}

#[tokio::test]
async fn missing_api_key_returns_an_error_without_a_request() {
    let fetch = RecordingFetch::new(vec![]);
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            fetch: Some(fetch.clone()),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = generate_images(model(), text_context(), options)
        .await
        .expect("generation returns a result");
    assert_eq!(output.stop_reason, ImagesStopReason::Error);
    assert_eq!(
        output.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from(
            "No API key for provider: openrouter"
        ))
    );
    assert!(fetch.requests().is_empty());
}

#[tokio::test]
async fn aborted_signal_returns_aborted_without_a_request() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::json(200, &success_body())]);
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    signal_controller.abort(None);
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some(("test".to_owned()).into()),
            fetch: Some(fetch.clone()),
            signal: Some(signal),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = generate_images(model(), text_context(), options)
        .await
        .expect("generation returns a result");
    assert_eq!(output.stop_reason, ImagesStopReason::Aborted);
    assert_eq!(
        output.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("Request was aborted."))
    );
    assert!(fetch.requests().is_empty());
}

async fn error_message_for(status: u16, headers: Vec<(String, String)>, body: &str) -> pi_js_compat::js_json::JsString {
    let fetch = RecordingFetch::new(vec![ScriptedResponse {
        status,
        headers,
        chunks: vec![body.as_bytes().to_vec()],
    }]);
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some(("test".to_owned()).into()),
            fetch: Some(fetch),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = generate_images(model(), text_context(), options)
        .await
        .expect("generation returns a result");
    assert_eq!(output.stop_reason, ImagesStopReason::Error);
    output.error_message.expect("error message")
}

#[tokio::test]
async fn http_errors_match_the_sdk_normalization() {
    let json_header = vec![("content-type".to_owned(), "application/json".to_owned())];
    assert_eq!(
        error_message_for(
            400,
            json_header.clone(),
            &json!({"error": {"message": "bad request"}}).to_string()
        )
        .await,
        "400: {\"message\":\"bad request\"}"
    );
    assert_eq!(
        error_message_for(500, vec![("content-type".to_owned(), "text/plain".to_owned())], "boom").await,
        "500 boom"
    );
    assert_eq!(
        error_message_for(429, json_header.clone(), "").await,
        "429 status code (no body)"
    );
    assert_eq!(
        error_message_for(
            401,
            json_header.clone(),
            &json!({"message": "unauthorized"}).to_string()
        )
        .await,
        "401 status code (no body)"
    );
    assert_eq!(
        error_message_for(502, json_header.clone(), &json!({"error": "string error"}).to_string()).await,
        "502 \"string error\""
    );
    assert_eq!(
        error_message_for(
            503,
            json_header,
            &json!({"error": {"message": "nested", "type": "x"}}).to_string()
        )
        .await,
        "503: {\"message\":\"nested\",\"type\":\"x\"}"
    );
}

#[tokio::test]
async fn retries_transient_failures_when_requested() {
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(500, "{}").with_header("retry-after-ms", "0"),
        ScriptedResponse::json(200, &success_body()),
    ]);
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some(("test".to_owned()).into()),
            fetch: Some(fetch.clone()),
            max_retries: Some(1.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = generate_images(model(), text_context(), options)
        .await
        .expect("generation returns a result");
    assert_eq!(output.stop_reason, ImagesStopReason::Stop);
    assert_eq!(fetch.requests().len(), 2);
}

#[tokio::test]
async fn on_payload_can_replace_the_request_body_and_on_response_sees_the_status() {
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(200, &success_body()).with_header("x-request-id", "r1"),
    ]);
    let on_payload = Arc::new(|mut payload: pi_js_compat::js_json::JsValue, _model: ImagesModel| {
        Box::pin(async move {
            payload["custom"] = pi_js_compat::js_json!(true);
            Ok(Some(payload))
        }) as BoxFuture<'static, Result<Option<pi_js_compat::js_json::JsValue>, CallbackError>>
    });
    let responses = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&responses);
    let on_response = Arc::new(move |response: ProviderResponse, _model: ImagesModel| {
        let recorder = Arc::clone(&recorder);
        Box::pin(async move {
            recorder.lock().expect("responses lock").push(response);
            Ok(())
        }) as BoxFuture<'static, Result<(), CallbackError>>
    });
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some(("test".to_owned()).into()),
            fetch: Some(fetch.clone()),
            on_payload: Some(on_payload),
            on_response: Some(on_response),
            ..Default::default()
        },
        ..Default::default()
    };
    generate_images(model(), text_context(), options)
        .await
        .expect("generation returns a result");
    let body: Value = serde_json::from_slice(fetch.requests()[0].body.as_deref().expect("body")).expect("JSON body");
    assert_eq!(body["custom"], json!(true));
    let responses = responses.lock().expect("responses lock");
    assert_eq!(responses[0].status, 200.0);
    assert_eq!(responses[0].headers.get("x-request-id").map(String::as_str), Some("r1"));
}

struct SlowFetch;

impl Fetch for SlowFetch {
    fn fetch(
        &self,
        _request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let signal = signal.expect("SDK supplies its timeout controller to fetch");
        Box::pin(async move { Err(CallbackError::from_rejection(signal.wait().await)) })
    }
}

#[tokio::test]
async fn timeout_returns_the_sdk_timeout_message() {
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some(("test".to_owned()).into()),
            fetch: Some(Arc::new(SlowFetch)),
            timeout_ms: Some(1.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = generate_images(model(), text_context(), options)
        .await
        .expect("generation returns a result");
    assert_eq!(output.stop_reason, ImagesStopReason::Error);
    assert_eq!(
        output.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("Request timed out."))
    );
}

#[tokio::test]
async fn sse_body_is_not_valid_non_streaming_json() {
    let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&["data: {}\n\n"])]);
    let output = generate_images(model(), text_context(), options_with_fetch("test", fetch.clone()))
        .await
        .expect("generation returns a result");
    assert_eq!(output.stop_reason, ImagesStopReason::Error);
    assert!(output.error_message.is_some());
    assert_eq!(fetch.requests().len(), 1);
}

#[tokio::test]
async fn abort_during_body_read_returns_aborted() {
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    let fetch = RecordingFetch::with_abort_after_chunks(
        vec![ScriptedResponse::json(200, &success_body())],
        0,
        signal_controller.clone(),
    );
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some(("test".to_owned()).into()),
            fetch: Some(fetch),
            signal: Some(signal),
            ..Default::default()
        },
        ..Default::default()
    };
    let output = generate_images(model(), text_context(), options)
        .await
        .expect("generation returns a result");
    assert_eq!(output.stop_reason, ImagesStopReason::Aborted);
    assert_eq!(
        output.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("Request was aborted."))
    );
}
