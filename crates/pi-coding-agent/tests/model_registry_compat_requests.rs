#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ProviderRequestOptions, StreamOptions,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::{ModelRegistry, ResolvedRequestAuth};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::{js_json, js_json::JsValue};
use serde_json::Value;

#[derive(Default)]
struct RequestCapture {
    requests: Mutex<Vec<JsValue>>,
}

struct FixtureBody(Option<Vec<u8>>);

impl HttpBody for FixtureBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.0.take();
        Box::pin(async move { chunk.map(Ok) })
    }
}

impl Fetch for RequestCapture {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let body = request.body.as_ref().expect("completion request has JSON body");
        let body = js_json::parse_lossless(std::str::from_utf8(body).unwrap()).unwrap();
        self.requests.lock().unwrap().push(body);
        Box::pin(async {
            Ok(HttpResponse {
                status:200,
                status_text:String::new(),
                headers:vec![("content-type".into(),"text/event-stream".into())],
                body:Box::new(FixtureBody(Some(b"data: {\"id\":\"fixture\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"fixture\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_vec()))),
            })
        })
    }
}

#[tokio::test]
async fn schema_accepted_compat_values_reach_the_exact_frozen_completion_request() {
    // Frozen completions1271/566: nullish detection then truthiness.
    // Frozen562: !== false. Frozen571-575: strict max_tokens equality; all
    // other values select max_completion_tokens without invoking ToString.
    let mut oracle = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let directory = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for (name, raw) in [
        ("store-null", r#"{"supportsStore":null}"#),
        ("store-zero", r#"{"supportsStore":0}"#),
        ("store-false", r#"{"supportsStore":false}"#),
        ("usage-zero", r#"{"supportsUsageInStreaming":0}"#),
        ("developer-null", r#"{"supportsDeveloperRole":null}"#),
        ("max-tokens-number", r#"{"maxTokensField":17}"#),
        ("max-tokens-object", r#"{"maxTokensField":{"toString":null}}"#),
    ] {
        let compat = js_json::parse_lossless(raw).unwrap();
        let config = js_json!({"providers":{"local":{
            "api":"openai-completions","apiKey":"private-fixture-key","baseUrl":"https://fixture.invalid",
            "models":[{"id":"fixture-model","compat":compat}]
        }}});
        let path = directory.path().join(format!("{name}.json"));
        std::fs::write(&path, js_json::stringify_lossless(&config)).unwrap();
        let expected: JsValue =
            oracle.request(&js_json!({"id":name,"operation":"config_request","modelsPath":path.to_str().unwrap()}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{name}: {expected:?}");
        if name.starts_with("max-tokens-") {
            assert_eq!(expected["output"]["errorMessage"], JsValue::Null);
            assert_eq!(
                expected["output"]["requests"][0]["max_completion_tokens"],
                JsValue::Number(11.0)
            );
        }
        let registry = ModelRegistry::create(
            AuthStorage::in_memory(Vec::<(String, Value)>::new()),
            Some(path.to_str().unwrap().into()),
        )
        .unwrap();
        let capture = Arc::new(RequestCapture::default());
        let actual = if let Some(model) = registry.find("local", "fixture-model") {
            let ResolvedRequestAuth::Ok { api_key, headers, env } =
                registry.get_api_key_and_headers(&model).await.unwrap()
            else {
                panic!("fixture auth must resolve");
            };
            let context = js_json::from_value(&js_json!({"systemPrompt":"fixture system","messages":[{"role":"user","content":"Hello","timestamp":1}]})).unwrap();
            let mut stream = pi_ai::api::openai_completions::stream(
                model,
                context,
                StreamOptions {
                    request: ProviderRequestOptions {
                        api_key,
                        fetch: Some(capture.clone()),
                        headers,
                        env,
                        ..Default::default()
                    },
                    max_tokens: Some(11.0),
                    ..Default::default()
                },
            );
            while let Some(event) = stream.next().await {
                event.unwrap();
            }
            let result = stream.result().await.unwrap();
            js_json!({"error":registry.get_error(),"requests":capture.requests.lock().unwrap().clone(),"errorMessage":result.error_message,"stopReason":result.stop_reason})
        } else {
            js_json!({"error":registry.get_error(),"requests":[],"modelMissing":true})
        };
        if actual != expected["output"] {
            eprintln!("compat consumer mismatch {name}");
            failures.push(js_json!({"case":name,"actual":actual,"expected":expected["output"]}));
        }
    }
    let log = std::env::temp_dir().join(format!("q15e-registry-compat-requests-{}.json", std::process::id()));
    std::fs::write(&log, js_json::stringify_lossless(&JsValue::Array(failures.clone()))).unwrap();
    assert!(
        failures.is_empty(),
        "{} compat request differences; {}",
        failures.len(),
        log.display()
    );
}
