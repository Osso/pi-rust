use crate::shared_oracle as oracle;

use pi_ai::model_catalog_generator::{Generator, HttpResponse};
use pi_js_compat::{
    js_json,
    js_json::{JsValue, stringify_lossless},
};

#[test]
fn openrouter_ignores_http_status_and_retains_frozen_cost_transform() {
    let body = js_json!({"data":[{"id":"quote\"slash\\","name":"literal","supported_parameters":["tools","reasoning"],"architecture":{"input_modalities":["text","image"]},"pricing":{"prompt":"0.0000012","completion":"0.0000024","input_cache_read":"0.0000003","input_cache_write":"0.0000008"},"context_length":0,"top_provider":{"max_completion_tokens":0}},{"id":"no-tools","supported_parameters":[]}]});
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let expected=source.request(&js_json!({"operation":"fetchOpenRouterModels","responses":{"https://openrouter.ai/api/v1/models":{"status":503,"body":body}}}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut fetch = |_: &str| {
        Ok(HttpResponse {
            status: 503,
            body: stringify_lossless(&body),
        })
    };
    let mut native = Generator::new(&mut fetch);
    let actual = native.fetch_open_router_models().unwrap();
    assert_eq!(actual, expected["result"], "source logs: {:?}", expected["logs"]);
    assert_eq!(JsValue::Array(native.logs), expected["logs"]);
}

#[test]
fn models_dev_catalog_uses_only_frozen_providers_and_fixed_order() {
    let model = js_json!({"name":"original","tool_call":true,"reasoning":true,"reasoning_options":[{"type":"effort","values":["low","high"]}],"cost":{"input":2,"output":3,"cache_read":0,"cache_write":1,"tiers":[{"tier":{"type":"context","size":100},"input":7}]},"limit":{"context":1024,"output":128},"modalities":{"input":["text","image"],"output":["text"]}});
    let mut data = js_json!({});
    let providers = [
        "qwen-token-plan",
        "baseten",
        "azure-openai-responses",
        "together-ai",
        "opencode-go",
        "opencode",
        "fireworks-ai",
        "huggingface",
        "mistral",
        "zai-coding-plan",
        "xai",
        "cloudflare-workers-ai",
        "cerebras",
        "groq",
        "openai",
        "google-vertex",
        "google",
        "anthropic",
        "amazon-bedrock",
        "minimax",
        "minimax-cn",
        "moonshotai",
        "moonshotai-cn",
        "xiaomi",
    ];
    for provider in providers {
        data[provider] = js_json!({"models":{"gemini-2.5-flash":model}});
    }
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let expected = source.request(
        &js_json!({"operation":"loadModelsDevData","responses":{"https://models.dev/api.json":{"body":data}}}),
    );
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut fetch = |_: &str| {
        Ok(HttpResponse {
            status: 200,
            body: stringify_lossless(&data),
        })
    };
    let mut native = Generator::new(&mut fetch);
    let actual = native.load_models_dev_data().unwrap();
    assert_eq!(actual, expected["result"]);
    assert_eq!(JsValue::Array(native.logs), expected["logs"]);
}
