#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::apply_model_metadata;
use pi_js_compat::js_json::{JsValue, parse_lossless, stringify_lossless};
use serde_json::json;

#[test]
fn source_metadata_branches_merge_existing_fields_in_source_order() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let operations = [
        "applyOpenAICompletionsCompatMetadata",
        "applyThinkingLevelMetadata",
        "applyStrictToolCompatMetadata",
        "applyOpenAIGrammarToolCompatMetadata",
        "applyOpenAIToolSearchMetadata",
        "applyOpenAIExplicitPromptCacheMetadata",
    ];
    let providers = [
        "openai",
        "openai-codex",
        "openai-codex-gc",
        "azure-openai-responses",
        "github-copilot",
        "opencode",
        "opencode-go",
        "cloudflare-ai-gateway",
        "cloudflare-workers-ai",
        "together",
        "deepseek",
        "zai",
        "zai-coding-cn",
        "moonshotai",
        "moonshotai-cn",
        "nvidia",
        "ant-ling",
        "cerebras",
        "xai",
        "openrouter",
        "anthropic",
        "fireworks",
        "groq",
        "kimi-coding",
        "vercel-ai-gateway",
    ];
    let ids = [
        "plain",
        "gpt-5.4",
        "gpt-5.5",
        "gpt-5.5-pro",
        "gpt-5.6-sol",
        "claude-opus-4.7",
        "claude-sonnet-4.6",
        "claude-fable-5",
        "deepseek-v4-flash",
        "gemini-3.1-pro",
        "gemini-flash-latest",
        "gemma-4-31b",
        "qwen/qwen3.6-27b",
        "kimi-k3",
        "k3",
        "moonshotai/kimi-k3",
        "~moonshotai/kimi-latest",
        "kimi-k2.7-code",
        "inception/mercury-2",
        "z-ai/glm-5.2",
        "glm-5.2",
        "accounts/glm-5p2",
        "kimi-k2.6",
        "grok-build-0.1",
        "grok-4.5",
        "anthropic/claude",
        "openai/gpt-5",
    ];
    for (index, provider) in providers.iter().enumerate() {
        for (id_index, id) in ids.iter().enumerate() {
            let api = [
                "openai-completions",
                "openai-responses",
                "anthropic-messages",
                "azure-openai-responses",
                "openai-codex-responses",
                "google-generative-ai",
                "google-vertex",
            ][(index + id_index) % 7];
            let input = json!({"provider":provider,"id":id,"api":api,"baseUrl":"https://example.invalid","reasoning":true,"cost":{"cacheWrite":1},"compat":{"extra":true},"thinkingLevelMap":{"high":"custom"}});
            let mut expected = input.clone();
            for operation in operations {
                let response = source.request(&json!({"operation":operation,"args":[expected],"mutated":true}));
                expected = serde_json::from_str(response["json"].as_str().unwrap()).unwrap();
            }
            let mut actual = JsValue::from(&input);
            apply_model_metadata(&mut actual).unwrap();
            assert_eq!(
                stringify_lossless(&actual),
                stringify_lossless(&parse_lossless(&serde_json::to_string(&expected).unwrap()).unwrap()),
                "{provider} {id} {api}"
            );
        }
    }
}
