#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::api::{
    anthropic_messages::AnthropicMessagesApi, azure_openai_responses::AzureOpenAIResponsesApi,
    google_generative_ai::GoogleGenerativeAiApi, google_vertex::GoogleVertexApi,
    mistral_conversations::MistralConversationsApi, openai_completions::OpenAICompletionsApi,
    openai_responses::OpenAIResponsesApi,
};
use pi_ai::request::{CallbackError, ProviderStreams, StreamOptions};
use pi_js_compat::{
    abort::AbortReason,
    js_json,
    js_json::{JsString, JsValue},
    node_error::NativeNodeError,
};
use std::sync::Arc;

fn normalize_clock(value: &mut JsValue) {
    match value {
        JsValue::Object(object) => {
            if let Some(timestamp) = object.get_mut("timestamp") {
                assert!(timestamp.as_f64().unwrap() > 0.0);
                *timestamp = js_json!(0);
            }
            for value in object.values_mut() {
                normalize_clock(value);
            }
        }
        JsValue::Array(values) => {
            for value in values {
                normalize_clock(value);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn provider_payload_catches_keep_raw_values_and_branded_error_fields() {
    let apis: Vec<(&str, Box<dyn ProviderStreams>)> = vec![
        ("anthropic-messages", Box::new(AnthropicMessagesApi)),
        ("azure-openai-responses", Box::new(AzureOpenAIResponsesApi)),
        ("google-generative-ai", Box::new(GoogleGenerativeAiApi)),
        ("google-vertex", Box::new(GoogleVertexApi)),
        ("mistral-conversations", Box::new(MistralConversationsApi)),
        ("openai-completions", Box::new(OpenAICompletionsApi)),
        ("openai-responses", Box::new(OpenAIResponsesApi)),
    ];
    let native = JsString::from("callback-") + JsString::from_utf16(vec![0xd800]);
    let cases = vec![
        ("value", JsValue::String(native.clone()), js_json!({})),
        ("value", js_json!({"message":native}), js_json!({})),
        (
            "error",
            js_json!("callback rejected"),
            js_json!({"status":429,"body":" server body "}),
        ),
        (
            "error",
            js_json!("callback rejected"),
            js_json!({"statusCode":422,"body":" validation body "}),
        ),
        (
            "error",
            js_json!("callback rejected"),
            js_json!({"error":{"metadata":{"raw":JsString::from("raw-\nprovider") + JsString::from_utf16(vec![0xd801])}}}),
        ),
    ];
    let mut mismatches = Vec::new();
    let apis = apis
        .into_iter()
        .map(|(api, implementation)| (api, Some(implementation)))
        .chain(std::iter::once(("openrouter-images", None)));
    for (api, implementation) in apis {
        let mut source = oracle::TsOracle::spawn("api-catch-oracle.ts", &format!("packages/ai/src/api/{api}.ts"));
        let model = js_json!({"id":"test","name":"test","api":api,"provider":"fixture-provider",
            "baseUrl":"https://test.openai.azure.com","reasoning":false,"input":["text"],"output":["text","image"],
            "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":8192,"maxTokens":2048});
        for (kind, value, properties) in &cases {
            let mut expected =
                source.request(&js_json!({"model":model,"kind":kind,"value":value,"properties":properties}));
            normalize_clock(&mut expected);
            assert_eq!(expected["requests"], js_json!(0));
            let rejection = if *kind == "error" {
                let mut error = NativeNodeError::new("Error", value.as_string().unwrap().clone());
                for (key, value) in properties.as_object().unwrap() {
                    error = error.with_property(key.clone(), value.clone());
                }
                CallbackError::from_rejection(AbortReason::Error(error))
            } else {
                CallbackError::from_rejection(AbortReason::Value(value.clone()))
            };
            let mut actual = if let Some(implementation) = implementation.as_ref() {
                let mut options = StreamOptions::default();
                options.request.api_key = Some("test-key".into());
                options.request.on_payload = Some(Arc::new(move |_, _| {
                    let rejection = rejection.clone();
                    Box::pin(async move { Err(rejection) })
                }));
                let mut stream = implementation
                    .stream(
                        pi_js_compat::js_json::from_value(&model).unwrap(),
                        pi_js_compat::js_json::from_value(&js_json!({"messages":[]})).unwrap(),
                        options,
                    )
                    .expect("raw acquisition");
                let mut events = Vec::new();
                while let Some(event) = stream.next().await {
                    events.push(event.unwrap());
                }
                js_json!({"requests":0,"events":events,"result":stream.result().await.unwrap()})
            } else {
                let mut options = pi_ai::request::ImagesOptions::default();
                options.request.api_key = Some("test-key".into());
                options.request.on_payload = Some(Arc::new(move |_, _| {
                    let rejection = rejection.clone();
                    Box::pin(async move { Err(rejection) })
                }));
                let result = pi_ai::api::openrouter_images::generate_images(
                    pi_js_compat::js_json::from_value(&model).unwrap(),
                    pi_js_compat::js_json::from_value(&js_json!({"input":[]})).unwrap(),
                    options,
                )
                .await
                .unwrap();
                js_json!({"requests":0,"events":[],"result":result})
            };
            normalize_clock(&mut actual);
            if actual != expected {
                mismatches.push(format!(
                    "{api}/{kind}: {} != {}",
                    pi_js_compat::js_json::stringify_lossless(&actual),
                    pi_js_compat::js_json::stringify_lossless(&expected)
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[tokio::test]
async fn vertex_payload_rejection_precedes_adc_credential_file_loading() {
    use pi_ai::api::google_vertex::GoogleVertexApi;
    let mut source = oracle::TsOracle::spawn("api-catch-oracle.ts", "packages/ai/src/api/google-vertex.ts");
    let model = js_json!({"id":"gemini-2.5-flash","name":"test","api":"google-vertex","provider":"google-vertex","baseUrl":"https://unused.invalid/v1","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":8192,"maxTokens":2048});
    let request_options = js_json!({"apiKey":"gcp-vertex-credentials","project":"fixture-project","location":"us-central1","env":{"GOOGLE_APPLICATION_CREDENTIALS":"/missing-pi-ai-api-credential.json"}});
    let mut expected = source.request(
        &js_json!({"model":model,"kind":"error","value":"payload wins","properties":{},"options":request_options}),
    );
    normalize_clock(&mut expected);
    assert_eq!(expected["requests"], js_json!(0));
    assert_eq!(expected["result"]["errorMessage"], js_json!("payload wins"));
    let mut options = StreamOptions {
        extra: serde_json::json!({"project":"fixture-project","location":"us-central1"})
            .as_object()
            .unwrap()
            .clone(),
        ..Default::default()
    };
    options.request.api_key = Some("gcp-vertex-credentials".into());
    options.request.env = Some(indexmap::IndexMap::from([(
        "GOOGLE_APPLICATION_CREDENTIALS".into(),
        "/missing-pi-ai-api-credential.json".into(),
    )]));
    options.request.on_payload = Some(Arc::new(|_, _| {
        Box::pin(async { Err(CallbackError::new("payload wins")) })
    }));
    let mut stream = GoogleVertexApi
        .stream(
            pi_js_compat::js_json::from_value(&model).unwrap(),
            pi_js_compat::js_json::from_value(&js_json!({"messages":[]})).unwrap(),
            options,
        )
        .unwrap();
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    let mut actual = js_json!({"requests":0,"events":events,"result":stream.result().await.unwrap()});
    normalize_clock(&mut actual);
    assert_eq!(actual, expected);
}
