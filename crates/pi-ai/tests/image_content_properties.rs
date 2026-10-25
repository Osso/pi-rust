use crate::shared_oracle as oracle;

use pi_ai::types::{Context, ImageContent, Model};
use pi_js_compat::js_json::{self, JsValue};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Capture(Mutex<Vec<pi_ai::request::HttpRequest>>);
impl pi_ai::request::Fetch for Capture {
    fn fetch(
        &self,
        request: pi_ai::request::HttpRequest,
        _: Option<pi_js_compat::abort::AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
        self.0.lock().unwrap().push(request);
        Box::pin(async { Err(pi_ai::request::CallbackError::new("captured")) })
    }
}

const IMAGE: &str = r#"{"before":{"\ud800":["\udfff",null]},"type":"image","middle":17,"data":"AA==","mimeType":"image/png","after":[{"nested":"\ud800"}]}"#;

#[test]
fn image_unknown_properties_and_order_match_frozen_message_json() {
    // Frozen ai/types.ts:363-367 declares known fields; message/session consumers keep the object.
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-content-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    let source = format!(r#"{{"role":"user","content":[{IMAGE}],"timestamp":1}}"#);
    let expected = oracle.request(&serde_json::json!({"wire": source}));
    let image: ImageContent = serde_json::from_str(IMAGE).unwrap();
    let serialized = serde_json::to_string(&image).unwrap();
    let expected_message = js_json::parse_lossless(expected["wire"].as_str().unwrap()).unwrap();
    assert_eq!(serialized, js_json::stringify_lossless(&expected_message["content"][0]));
    let mut changed = image;
    changed.data = "BB==".into();
    changed.mime_type = "image/jpeg".into();
    let mut expected_image = js_json::parse_lossless(IMAGE).unwrap();
    let JsValue::Object(ref mut fields) = expected_image else {
        unreachable!()
    };
    fields.insert("data".into(), JsValue::String("BB==".into()));
    fields.insert("mimeType".into(), JsValue::String("image/jpeg".into()));
    assert_eq!(
        serde_json::to_string(&changed).unwrap(),
        js_json::stringify_lossless(&expected_image)
    );
}

#[test]
fn image_unknown_native_infinity_survives_before_json_output() {
    let mut native = js_json::parse_lossless(IMAGE).unwrap();
    let JsValue::Object(ref mut fields) = native else {
        unreachable!()
    };
    fields.insert("middle".into(), JsValue::Number(f64::INFINITY));
    let image: ImageContent = js_json::from_value(&native).unwrap();
    // to_value is a JSON boundary, not a native-value serializer (js_json.rs:591-593).
    // Inspect the actual stored authority before JSON converts nonfinite numbers.
    assert_eq!(
        image.properties.extra().get("middle"),
        Some(&JsValue::Number(f64::INFINITY))
    );
    assert_eq!(image.properties.extra().get("before"), native.get("before"));
    assert_eq!(image.properties.extra().get("after"), native.get("after"));
    for known in ["type", "data", "mimeType"] {
        assert!(!image.properties.extra().contains_key(known));
    }
    let mut cloned = image.clone();
    cloned.data = "BB==".into();
    assert_eq!(cloned.properties.extra(), image.properties.extra());
    let json = serde_json::to_string(&cloned).unwrap();
    assert_eq!(js_json::parse_lossless(&json).unwrap()["middle"], JsValue::Null);
}

#[tokio::test]
async fn provider_mapping_never_forwards_image_unknown_properties() {
    // Frozen openai-completions.ts:905-910 constructs an image_url from known fields.
    let model: Model = serde_json::from_value(serde_json::json!({
        "id":"gpt-test","name":"fixture","api":"openai-completions","provider":"openai",
        "baseUrl":"https://fixture.invalid/v1","reasoning":false,"input":["text","image"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":1024,"maxTokens":32
    }))
    .unwrap();
    let raw = js_json::parse_lossless(format!(
        r#"{{"messages":[{{"role":"user","content":[{IMAGE}],"timestamp":1}}]}}"#
    ))
    .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "openai-responses-oracle.ts",
        "packages/ai/src/api/openai-completions.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"id":"image-properties","operation":"stream","model":model,"context":raw,"stream":{"apiKey":"fixture","maxRetries":0},"extra":{},"responses":[{"status":400,"headers":{"content-type":"application/json"},"chunks":["{\"error\":{\"message\":\"captured\"}}"]}]}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let capture = Arc::new(Capture::default());
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.fetch = Some(capture.clone());
    options.request.api_key = Some("fixture".into());
    options.request.max_retries = Some(0.0);
    let context: Context = js_json::from_value(&raw).unwrap();
    let _ = pi_ai::api::openai_completions::stream(model, context, options)
        .result()
        .await;
    let requests = capture.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let body = js_json::parse_lossless(std::str::from_utf8(requests[0].body.as_ref().unwrap()).unwrap()).unwrap();
    let expected_body = &expected["output"]["requests"][0]["body"];
    let mapped = pi_js_compat::js_json!([{"type":"image_url","image_url":{"url":"data:image/png;base64,AA=="}}]);
    assert_eq!(body["messages"][0]["content"], mapped);
    assert_eq!(body["messages"], expected_body["messages"]);
}
