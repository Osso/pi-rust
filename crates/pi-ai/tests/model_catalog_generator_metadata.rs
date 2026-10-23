#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::apply_model_metadata;
use pi_js_compat::js_json::{JsValue, parse_lossless, stringify_lossless};
use serde_json::json;

#[test]
fn source_raw_cache_coercion_and_ascii_google_patterns_match() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let mut failures = vec![];
    let operations = [
        "applyOpenAICompletionsCompatMetadata",
        "applyThinkingLevelMetadata",
        "applyStrictToolCompatMetadata",
        "applyOpenAIGrammarToolCompatMetadata",
        "applyOpenAIToolSearchMetadata",
        "applyOpenAIExplicitPromptCacheMetadata",
    ];
    let mut inputs = vec![];
    for cache in [
        json!(0),
        json!("2"),
        json!(true),
        json!(["2"]),
        json!({"toString":false}),
        json!({}),
        json!("bad"),
    ] {
        inputs.push(json!({"provider":"openai","id":"custom","api":"openai-responses","baseUrl":"https://api.openai.com/v1","cost":{"cacheWrite":cache}}));
    }
    for api in ["google-generative-ai", "google-vertex"] {
        for id in ["gemini-3.१-pro", "GEMINI-3.1-PRO", "gemini-3.١-flash", "gemma-4-model"] {
            inputs.push(json!({"provider":"google","id":id,"api":api,"baseUrl":"https://example.invalid","cost":{"cacheWrite":0}}));
        }
    }
    for input in inputs {
        let mut expected = input.clone();
        let mut expected_error = None;
        for operation in operations {
            let response = source.request(&json!({"operation":operation,"args":[expected],"mutated":true}));
            if let Some(error) = response.get("error") {
                expected_error = Some(error.clone());
                break;
            }
            expected = serde_json::from_str(response["json"].as_str().unwrap()).unwrap();
        }
        let mut actual = JsValue::from(&input);
        let result = apply_model_metadata(&mut actual);
        if let Some(error) = expected_error {
            match result {
                Ok(()) => failures.push(format!("{input}: missing source error {error}")),
                Err(actual) => {
                    if actual.name != error["name"].as_str().unwrap()
                        || actual.message.to_utf8().unwrap() != error["message"].as_str().unwrap()
                    {
                        failures.push(format!("{input}: wrong source error {actual}"));
                    }
                }
            }
        } else if result.is_err() || stringify_lossless(&actual) != stringify_lossless(&JsValue::from(&expected)) {
            failures.push(format!(
                "{input}: actual {} {:?}; expected {expected}",
                stringify_lossless(&actual),
                result
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

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
                stringify_lossless(&parse_lossless(serde_json::to_string(&expected).unwrap()).unwrap()),
                "{provider} {id} {api}"
            );
        }
    }
}

#[test]
fn custom_provider_url_routes_and_empty_compat_deletion_match_source() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for endpoint in [
        "example.invalid",
        "api.z.ai",
        "open.bigmodel.cn",
        "api.together.ai",
        "api.together.xyz",
        "api.moonshot.cn",
        "openrouter.ai",
        "api.cloudflare.com",
        "gateway.ai.cloudflare.com",
        "integrate.api.nvidia.com",
        "api.ant-ling.com",
        "API.DEEPSEEK.COM",
        "cerebras.ai",
        "api.x.ai",
        "chutes.ai",
        "opencode.ai",
    ] {
        for id in ["plain", "anthropic/claude", "deepseek-ai/DeepSeek-R1"] {
            for compat in [None, Some(json!({})), Some(json!({"extra":true}))] {
                let mut input = json!({"provider":"custom","id":id,"api":"openai-completions","baseUrl":format!("https://{endpoint}/v1"),"cost":{"cacheWrite":0}});
                if let Some(compat) = compat {
                    input["compat"] = compat;
                }
                let expected = source.request(
                    &json!({"operation":"applyOpenAICompletionsCompatMetadata","args":[input],"mutated":true}),
                );
                let expected = parse_lossless(expected["json"].as_str().unwrap()).unwrap();
                let mut actual = JsValue::from(&input);
                apply_model_metadata(&mut actual).unwrap();
                assert_eq!(stringify_lossless(&actual), stringify_lossless(&expected), "{input}");
                if endpoint == "example.invalid"
                    && input
                        .get("compat")
                        .is_none_or(|compat| compat.as_object().unwrap().is_empty())
                {
                    let JsValue::Object(fields) = actual else {
                        panic!("model object")
                    };
                    assert!(
                        !fields.contains_key(&pi_js_compat::js_json::JsString::from("compat")),
                        "empty compat must be omitted"
                    );
                }
            }
        }
    }
}
