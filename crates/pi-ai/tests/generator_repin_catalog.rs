#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::model_catalog_generator::{Generator, GeneratorOptions, HttpResponse};
use pi_js_compat::{
    js_json,
    js_json::{JsValue, stringify_lossless},
};

#[test]
fn final_catalog_matches_frozen_overrides_supplements_and_source_priority() {
    let entry = js_json!({"name":"source","tool_call":true,"reasoning":true,"cost":{"input":3,"output":7,"cache_read":1,"cache_write":2},"limit":{"context":4567,"output":89},"modalities":{"input":["text","image"]}});
    let mut data = js_json!({});
    for provider in [
        "openai",
        "anthropic",
        "opencode",
        "opencode-go",
        "github-copilot",
        "fireworks-ai",
        "minimax",
        "minimax-cn",
        "xai",
    ] {
        for id in [
            "gpt-5-chat-latest",
            "gpt-5.4",
            "gpt-5.5",
            "gpt-5.6-luna",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5-pro",
            "gpt-6-astra",
            "gpt-5.3-codex-spark",
            "claude-opus-4.6",
            "claude-opus-4.7",
            "claude-opus-5-5",
            "claude-sonnet-5-5",
            "claude-haiku-5-5",
            "claude-sonnet-4",
            "glm-5p2",
            "kimi-k3",
            "MiniMax-M2.7",
            "MiniMax-M3",
            "grok-3",
            "grok-3-fast",
            "grok-code-fast-1",
        ] {
            if matches!(data[provider], JsValue::Null) {
                data[provider] = js_json!({"models":{}});
            }
            data[provider]["models"][id] = entry.clone();
        }
    }
    let responses = js_json!({"https://models.dev/api.json":{"body":data},"https://openrouter.ai/api/v1/models":{"body":{"data":[]}},"https://ai-gateway.vercel.sh/v1/models":{"body":{"data":[]}}});
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let expected = source.request(&js_json!({"operation":"workflow","responses":responses}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut fetch = |url: &str| {
        Ok(HttpResponse {
            status: 200,
            body: stringify_lossless(&responses[url]["body"]),
        })
    };
    let mut native = Generator::new(GeneratorOptions::default(), &mut fetch);
    let actual = native.generate_catalog().unwrap();
    let actual_object = actual.as_object().unwrap();
    let expected_object = expected["catalog"].as_object().unwrap();
    assert_eq!(
        actual_object.keys().collect::<Vec<_>>(),
        expected_object.keys().collect::<Vec<_>>(),
        "provider inventory"
    );
    for (provider, expected_models) in expected_object {
        let actual_models = &actual_object[provider];
        assert_eq!(
            actual_models.as_object().unwrap().keys().collect::<Vec<_>>(),
            expected_models.as_object().unwrap().keys().collect::<Vec<_>>(),
            "provider={provider:?}"
        );
        for (id, expected_model) in expected_models.as_object().unwrap() {
            assert_eq!(
                stringify_lossless(&actual_models.as_object().unwrap()[id]),
                stringify_lossless(expected_model),
                "provider={provider:?},id={id:?}"
            );
        }
    }
}
