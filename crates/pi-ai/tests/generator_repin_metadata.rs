//! Frozen generate-models.ts:328–589 metadata, actual module via AST exposure.
use crate::shared_oracle as oracle;
use pi_ai::model_catalog_generator::apply_model_metadata;
use pi_js_compat::{js_json, js_json::JsValue};

#[test]
fn metadata_matches_frozen_common_pin_without_old_only_fields() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let models = js_json!([
        {"provider":"deepseek","id":"deepseek-v4-flash","api":"openai-completions","baseUrl":"https://api.deepseek.com","reasoning":true,"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0}},
        {"provider":"openai-codex-team","id":"gpt-6-astra","api":"openai-codex-responses","baseUrl":"https://chatgpt.com/backend-api","reasoning":true,"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0}},
        {"provider":"anthropic","id":"claude-sonnet-5-5","api":"anthropic-messages","baseUrl":"https://api.anthropic.com","reasoning":true,"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0}}
    ]);
    let expected = source.request(&js_json!({"operation":"metadata","models":models}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut actual = models;
    for model in actual.as_array_mut().unwrap() {
        apply_model_metadata(model).unwrap();
    }
    assert_eq!(actual, expected["result"]);
}

#[test]
fn frozen_metadata_branch_matrix_preserves_order_and_utf16() {
    let providers = [
        "openai",
        "openai-codex",
        "openai-codex-gc",
        "openai-codex-team",
        "anthropic",
        "github-copilot",
        "openrouter",
        "deepseek",
        "opencode",
        "opencode-go",
        "google",
        "google-vertex",
        "groq",
        "moonshotai",
        "moonshotai-cn",
        "fireworks",
        "ant-ling",
        "zai",
        "zai-coding-cn",
        "together",
        "nvidia",
        "cloudflare-workers-ai",
        "cloudflare-ai-gateway",
        "xai",
    ];
    let ids = [
        "plain",
        "gpt-5.1",
        "gpt-5.4",
        "gpt-5.5",
        "gpt-5.5-pro",
        "gpt-5.6-sol",
        "gpt-6-astra",
        "claude-opus-4.6",
        "claude-opus-4.7",
        "claude-opus-4.8",
        "claude-sonnet-4.6",
        "claude-sonnet-5-5",
        "claude-haiku-5.5",
        "claude-fable-5",
        "mythos-5",
        "deepseek-v4-flash",
        "gemini-3.7-flash",
        "GEMINI-3.1-PRO",
        "gemini-3.१-pro",
        "gemini-flash-lite-latest",
        "gemma-4-model",
        "qwen/qwen3-32b",
        "kimi-k2.7-code",
        "kimi-k2.7-code-highspeed",
        "inception/mercury-2",
        "z-ai/glm-5.2",
        "glm-5p2",
        "glm-5.2",
        "kimi-k2.6",
        "grok-build-0.1",
        "deepseek-ai/DeepSeek-R1",
        "MiniMaxAI/MiniMax-M2.7",
        "anthropic/test",
        "~anthropic/test",
        "openai/test",
    ];
    let apis = [
        "openai-completions",
        "openai-responses",
        "openai-codex-responses",
        "anthropic-messages",
        "bedrock-converse-stream",
        "google-generative-ai",
        "google-vertex",
    ];
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let mut failures = Vec::new();
    for provider in providers {
        let mut models = Vec::new();
        for api in apis {
            for id in ids {
                models.push(js_json!({"id":id,"provider":provider,"api":api,"baseUrl":"https://fixture.invalid","reasoning":true,"thinkingLevelMap":{"existing":pi_js_compat::js_json::JsString::from_utf16(vec![0xd800])},"compat":{"sentinel":pi_js_compat::js_json::JsString::from_utf16(vec![0xdc00])},"name":"quote\"slash\\"}));
            }
        }
        let expected = source.request(&js_json!({"operation":"metadata","models":models}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        for (mut model, expected) in models.into_iter().zip(expected["result"].as_array().unwrap()) {
            apply_model_metadata(&mut model).unwrap();
            // Metadata object key insertion order is observable in generated JSON.
            if pi_js_compat::js_json::stringify_lossless(&model) != pi_js_compat::js_json::stringify_lossless(expected)
            {
                failures.push(js_json!({"actual":model,"expected":expected}));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} cases differ; first={:?}",
        failures.len(),
        failures.first()
    );
}
