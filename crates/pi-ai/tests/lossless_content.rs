#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::types::ToolResultMessage;
use pi_js_compat::js_json::parse_lossless;

#[test]
fn tool_message_round_trip_keeps_text_details_and_null_presence() {
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-content-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    for details in ["", r#", "details":null"#, r#", "details":{"\ud800":"\udfff"}"#] {
        let source = format!(
            r#"{{"role":"toolResult","toolCallId":"call","toolName":"probe","content":[{{"type":"text","text":"x\ud800\udfff\ud800"}}]{details},"isError":false,"timestamp":1}}"#
        );
        let message: ToolResultMessage = serde_json::from_str(&source).unwrap();
        let output = serde_json::to_string(&message).unwrap();
        assert!(message.content.iter().any(|part| match part {
            pi_ai::types::TextOrImageContent::Text(text) => text.text_utf8().is_err(),
            _ => false,
        }));
        let reference = oracle.request(&serde_json::json!({"wire": source}));
        assert_eq!(reference["units"], serde_json::json!([120, 0xd800, 0xdfff, 0xd800]));
        let reverse = oracle.request(&serde_json::json!({"wire": output}));
        assert_eq!(reverse["units"], reference["units"]);
        assert_eq!(reverse["wire"], output);
        assert_eq!(
            pi_ai::utils::sanitize_unicode::sanitize_surrogates(&pi_ai::utils::text::content_text(
                pi_ai::utils::text::ContentText::TextOrImage(&message.content),
                "\n"
            )),
            reference["sanitized"].as_str().unwrap()
        );
        assert_eq!(parse_lossless(&output).unwrap(), parse_lossless(&source).unwrap());
    }
}

#[derive(Default)]
struct CaptureRequest(std::sync::Mutex<Vec<pi_ai::request::HttpRequest>>);
impl pi_ai::request::Fetch for CaptureRequest {
    fn fetch(
        &self,
        request: pi_ai::request::HttpRequest,
        _signal: Option<tokio_util::sync::CancellationToken>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
        self.0.lock().unwrap().push(request);
        Box::pin(async { Err(pi_ai::request::CallbackError::new("captured request")) })
    }
}

#[tokio::test]
async fn pi_messages_http_json_and_payload_callback_keep_utf16_escapes() {
    use pi_ai::types::{Context, Message, Model};
    use pi_js_compat::js_json::{JsString, JsValue};
    let message: ToolResultMessage = serde_json::from_str(r#"{"role":"toolResult","toolCallId":"call","toolName":"probe","content":[{"type":"text","text":"\ud800"}],"details":{"x":"\udfff"},"isError":false,"timestamp":1}"#).unwrap();
    let model: Model = serde_json::from_value(serde_json::json!({
        "id":"probe", "name":"probe", "api":"pi-messages", "provider":"probe", "baseUrl":"https://provider.invalid/v1", "reasoning":false, "input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}, "contextWindow":10000, "maxTokens":1000
    })).unwrap();
    let fetch = std::sync::Arc::new(CaptureRequest::default());
    let mut options = pi_ai::api::pi_messages::PiMessagesOptions::default();
    options.stream.request.fetch = Some(fetch.clone());
    options.stream.request.api_key = Some("test".into());
    options.stream.request.max_retries = Some(0.0);
    options.stream.request.on_payload = Some(std::sync::Arc::new(|mut payload, _| {
        Box::pin(async move {
            payload
                .as_object_mut()
                .unwrap()
                .insert("callback".into(), JsValue::String(JsString::from_utf16(vec![0xdfff])));
            Ok(Some(payload))
        })
    }));
    let mut stream = pi_ai::api::pi_messages::stream(
        model,
        Context {
            system_prompt: None,
            messages: vec![Message::ToolResult(message)],
            tools: None,
        },
        options,
    );
    while stream.next().await.is_some() {}
    let requests = fetch.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let body = std::str::from_utf8(requests[0].body.as_ref().unwrap()).unwrap();
    assert!(body.contains(r#""text":"\ud800""#));
    assert!(body.contains(r#""details":{"x":"\udfff"}"#));
    assert!(body.contains(r#""callback":"\udfff""#));
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-content-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    let reference = oracle.request(&serde_json::json!({"wire":body}));
    assert_eq!(reference["wire"], body);
}
