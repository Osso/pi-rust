use crate::shared_oracle as oracle;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse, StreamOptions};
use pi_ai::types::{Context, Model};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::parse_lossless;
use std::sync::{Arc, Mutex};

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

fn model() -> Model {
    serde_json::from_value(serde_json::json!({
        "id":"claude-test","name":"fixture","api":"anthropic-messages","provider":"anthropic",
        "baseUrl":"https://provider.invalid","reasoning":false,"input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000
    }))
    .unwrap()
}

const METADATA: &str = r#"{"user_id":"before\ud800after","\udfff":{"nested":[null,3,"\udc00"]}}"#;

#[tokio::test]
async fn anthropic_metadata_user_id_matches_frozen_request_without_utf16_projection() {
    let mut oracle = oracle::TsOracle::spawn(
        "anthropic-messages-oracle.ts",
        "packages/ai/src/api/anthropic-messages.ts",
    );
    let metadata = parse_lossless(METADATA).unwrap();
    let expected = oracle.request(&pi_js_compat::js_json!({
        "id":1,"kind":"stream","model":model(),"context":{"messages":[]},
        "options":{"apiKey":"fixture","maxRetries":0,"metadata":metadata},
        "script":[{"status":400,"headers":[["content-type","application/json"]],"chunks":["{\"error\":{\"message\":\"captured\"}}"]}]
    }));
    assert_eq!(expected["ok"].as_bool(), Some(true), "{expected:?}");
    let mut options = StreamOptions {
        metadata: Some(serde_json::from_str(METADATA).expect("metadata must accept arbitrary UTF-16 JSON")),
        ..Default::default()
    };
    options.request.api_key = Some("fixture".into());
    options.request.max_retries = Some(0.0);
    let fetch = Arc::new(Capture::default());
    options.request.fetch = Some(fetch.clone());
    let stream = pi_ai::api::anthropic_messages::stream(
        model(),
        Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        options,
    );
    let _ = stream.result().await;
    let requests = fetch.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let actual = parse_lossless(std::str::from_utf8(requests[0].body.as_ref().unwrap()).unwrap()).unwrap();
    let source_body = expected["output"]["requests"][0]["body"]
        .as_string()
        .unwrap()
        .to_utf8()
        .unwrap();
    let source = parse_lossless(&source_body).unwrap();
    assert_eq!(actual["metadata"], source["metadata"]);
    assert_eq!(actual["metadata"]["user_id"], metadata["user_id"]);
}

#[test]
fn simple_options_keep_all_nested_metadata_code_units() {
    let options = StreamOptions {
        metadata: Some(serde_json::from_str(METADATA).expect("native metadata")),
        ..Default::default()
    };
    let expected = parse_lossless(METADATA).unwrap();
    let simple = pi_ai::request::SimpleStreamOptions {
        stream: options,
        ..Default::default()
    };
    let base = pi_ai::api::simple_options::build_base_options(
        &model(),
        &Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        Some(&simple),
        Some(&"key".into()),
    );
    assert_eq!(pi_js_compat::js_json::JsValue::Object(base.metadata.unwrap()), expected);
}
