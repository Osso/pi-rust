use crate::shared_oracle as oracle;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, StreamOptions};
use pi_ai::types::{Context, Model};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue, from_value, parse_lossless};
use std::sync::{Arc, Mutex};

struct EmptyBody;
impl HttpBody for EmptyBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { None })
    }
}

#[derive(Default)]
struct Capture(Mutex<Vec<HttpRequest>>);
impl Fetch for Capture {
    fn fetch(
        &self,
        request: HttpRequest,
        _: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(HttpResponse {
                status: 400,
                status_text: "Bad Request".into(),
                headers: vec![("content-type".into(), "application/json".into())],
                body: Box::new(EmptyBody),
            })
        })
    }
}

const APIS: &[(&str, &str)] = &[
    ("anthropic-messages", "anthropic"),
    ("openai-completions", "openai"),
    ("openai-responses", "openai"),
    ("azure-openai-responses", "azure-openai-responses"),
    ("openai-codex-responses", "openai-codex"),
    ("mistral-conversations", "mistral"),
    ("google-generative-ai", "google"),
    ("google-vertex", "google-vertex"),
    ("bedrock-converse-stream", "amazon-bedrock"),
];

fn model(api: &str, provider: &str) -> Model {
    serde_json::from_value(serde_json::json!({
        "id":"probe","name":"probe","api":api,"provider":provider,
        "baseUrl": if api == "azure-openai-responses" {"https://example.openai.azure.com"} else {"https://provider.invalid"},
        "reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},
        "contextWindow":10000,"maxTokens":1000
    })).unwrap()
}

fn codex_key() -> String {
    let payload = pi_js_compat::base64::btoa(br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acc_test"}}"#)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_");
    format!("header.{payload}.signature")
}

fn decode_body(value: &JsValue) -> JsValue {
    match value.as_string() {
        Some(body) => parse_lossless(body.to_utf8().unwrap()).unwrap(),
        None => value.clone(),
    }
}

fn source_body(api: &str, model: &Model, context: &JsValue) -> JsValue {
    let module = format!("packages/ai/src/api/{api}.ts");
    let (script, source) = match api {
        "anthropic-messages" => ("anthropic-messages-oracle.ts", module),
        "openai-codex-responses" => ("openai-codex-responses-oracle.ts", "packages/ai/src".into()),
        "mistral-conversations" => ("mistral-conversations-oracle.ts", module),
        "google-generative-ai" | "google-vertex" => ("google-api-oracle.ts", "packages/ai/src/api".into()),
        "bedrock-converse-stream" => ("bedrock-converse-stream-oracle.ts", "packages/ai/src".into()),
        _ => ("openai-responses-oracle.ts", module),
    };
    let mut oracle = oracle::TsOracle::spawn(script, &source);
    let request = match api {
        "anthropic-messages" => pi_js_compat::js_json!({
            "id":1,"kind":"stream","model":model,"context":context,
            "options":{"apiKey":"key","maxRetries":0},
            "script":[{"status":400,"headers":[["content-type","application/json"]],"chunks":["{\"error\":{\"message\":\"captured\"}}"]}]
        }),
        "openai-codex-responses" => pi_js_compat::js_json!({
            "id":1,"operation":"sse","events":[],"maxRetries":0,"modelOverrides":model,"context":context
        }),
        "mistral-conversations" => pi_js_compat::js_json!({
            "id":1,"api":"stream","model":model,"context":context,"options":{"apiKey":"key","maxRetries":0},
            "response":{"status":400,"headers":{"content-type":"application/json"},"chunks":["{\"message\":\"captured\"}"]}
        }),
        "bedrock-converse-stream" => pi_js_compat::js_json!({
            "id":1,"model":model,"context":context,"options":{"bearerToken":"key","maxRetries":0,"region":"us-east-1"},
            "script":[{"status":400,"headers":[["content-type","application/json"]],"text":"{\"message\":\"captured\"}"}]
        }),
        _ => pi_js_compat::js_json!({
            "id":1,"api":api,"operation":"stream","model":model,"context":context,
            "stream":{"apiKey":"key","maxRetries":0},
            "responses":[{"status":400,"headers":{"content-type":"application/json"},"chunks":["{\"error\":{\"message\":\"captured\"}}"]}]
        }),
    };
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"].as_bool(), Some(true), "{api}: {expected:?}");
    if api == "mistral-conversations" {
        decode_body(&expected["request"]["body"])
    } else {
        decode_body(&expected["output"]["requests"][0]["body"])
    }
}

fn description<'a>(api: &str, body: &'a JsValue) -> &'a JsString {
    let value = match api {
        "openai-completions" | "mistral-conversations" => &body["tools"][0]["function"]["description"],
        "google-generative-ai" | "google-vertex" => &body["tools"][0]["functionDeclarations"][0]["description"],
        "bedrock-converse-stream" => &body["toolConfig"]["tools"][0]["toolSpec"]["description"],
        _ => &body["tools"][0]["description"],
    };
    value
        .as_string()
        .unwrap_or_else(|| panic!("{api}: missing tool description in {body:?}"))
}

fn stream(
    model: Model,
    context: Context,
    options: StreamOptions,
) -> pi_ai::utils::event_stream::AssistantMessageEventStream {
    match model.api.as_str() {
        "anthropic-messages" => pi_ai::api::anthropic_messages::stream(model, context, options),
        "openai-completions" => pi_ai::api::openai_completions::stream(model, context, options),
        "openai-responses" => pi_ai::api::openai_responses::stream(model, context, options),
        "azure-openai-responses" => pi_ai::api::azure_openai_responses::stream(model, context, options),
        "openai-codex-responses" => pi_ai::api::openai_codex_responses::stream(
            model,
            context,
            pi_ai::api::openai_codex_responses::OpenAICodexResponsesOptions::from_stream_options(options).unwrap(),
        ),
        "mistral-conversations" => pi_ai::api::mistral_conversations::stream(
            model,
            context,
            pi_ai::api::mistral_conversations::MistralOptions::from_stream_options(options),
        ),
        "google-generative-ai" => pi_ai::api::google_generative_ai::stream(model, context, options),
        "google-vertex" => pi_ai::api::google_vertex::stream(model, context, options),
        "bedrock-converse-stream" => pi_ai::api::bedrock_converse_stream::stream(model, context, options),
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn unmatched_tool_descriptions_reach_all_frozen_provider_requests() {
    let fixture = parse_lossless(r#"{"messages":[{"role":"user","content":"hi","timestamp":0}],"tools":[{"name":"lookup","description":"human\ud800text\udfff","parameters":{"type":"object","properties":{}}}]}"#).unwrap();
    let input = fixture["tools"][0]["description"].as_string().unwrap();
    for (api, provider) in APIS {
        let model = model(api, provider);
        let expected = source_body(api, &model, &fixture);
        assert_eq!(
            description(api, &expected),
            input,
            "{api}: frozen request keeps unmatched units"
        );
        let context: Context =
            from_value(&fixture).expect("tool descriptions are human UTF-16 text, not scalar metadata");
        let fetch = Arc::new(Capture::default());
        let mut options = StreamOptions::default();
        options.request.api_key = Some(if *api == "openai-codex-responses" {
            codex_key().into()
        } else {
            "key".into()
        });
        options.request.max_retries = Some(0.0);
        options.request.fetch = Some(fetch.clone());
        if *api == "bedrock-converse-stream" {
            options.extra.insert("region".into(), serde_json::json!("us-east-1"));
            options.extra.insert("bearerToken".into(), serde_json::json!("key"));
        }
        options.transport = Some(pi_ai::types::Transport::Sse);
        let result = stream(model, context, options).result().await.unwrap();
        let requests = fetch.0.lock().unwrap();
        assert_eq!(requests.len(), 1, "{api}: {:?}", result.error_message);
        let request = &requests[0];
        let bytes = request.body.as_ref().unwrap();
        let decoded = if request
            .headers
            .iter()
            .any(|(name, value)| name.eq_ignore_ascii_case("content-encoding") && value == "zstd")
        {
            zstd::stream::decode_all(bytes.as_slice()).unwrap()
        } else {
            bytes.clone()
        };
        let actual = parse_lossless(std::str::from_utf8(&decoded).unwrap()).unwrap();
        assert_eq!(
            description(api, &actual),
            description(api, &expected),
            "{api}: outbound human description"
        );
    }
}
