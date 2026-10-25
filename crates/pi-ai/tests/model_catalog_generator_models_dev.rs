use crate::shared_oracle as oracle;
use pi_ai::model_catalog_generator::{Generator, HttpResponse};
use pi_js_compat::{js_json, js_json::stringify_lossless};
use serde_json::{Value, json};

fn fixture() -> Value {
    let mut data = json!({});
    for provider in [
        "amazon-bedrock",
        "anthropic",
        "google",
        "google-vertex",
        "openai",
        "groq",
        "cerebras",
        "cloudflare-workers-ai",
        "cloudflare-ai-gateway",
        "xai",
        "zai-coding-plan",
        "mistral",
        "huggingface",
        "fireworks-ai",
        "nvidia",
        "together",
        "baseten",
        "opencode",
        "opencode-go",
        "github-copilot",
        "minimax",
        "minimax-cn",
        "kimi-for-coding",
        "moonshotai",
        "moonshotai-cn",
        "xiaomi",
        "xiaomi-token-plan-cn",
        "xiaomi-token-plan-ams",
        "xiaomi-token-plan-sgp",
        "alibaba-token-plan",
        "alibaba-token-plan-cn",
    ] {
        data[provider] = json!({"models":{"plain":{"tool_call":true,"name":"Example","reasoning":true,"reasoning_options":[{"type":"effort","values":["low","high"]}],"modalities":{"input":["text","image"],"output":["text"]},"cost":{"input":1.25,"output":2,"cache_write":0.5},"limit":{"context":8192,"output":1024}},"no-tools":{"tool_call":false},"deprecated":{"tool_call":true,"status":"deprecated"}}});
    }
    for (provider, ids) in [
        (
            "amazon-bedrock",
            vec![
                "ai21.jamba-x",
                "mistral.mistral-7b-instruct-v0-x",
                "anthropic.claude-opus-5",
                "eu.anthropic.claude",
            ],
        ),
        (
            "google",
            vec![
                "gemini-flash-latest",
                "gemini-3.5-flash",
                "gemini-flash-lite-latest",
                "gemini-3.1-flash-lite",
            ],
        ),
        (
            "google-vertex",
            vec![
                "gemini-flash-latest",
                "gemini-3.5-flash",
                "gemini-2.5-flash",
                "gemini-3.1-flash-lite-preview",
            ],
        ),
        (
            "cloudflare-ai-gateway",
            vec![
                "openai/gpt-5.4",
                "anthropic/claude",
                "workers-ai/@cf/test",
                "other/skip",
                "no-slash",
            ],
        ),
        ("xai", vec!["grok-4.5"]),
        ("openai", vec!["gpt-5.6"]),
        (
            "zai-coding-plan",
            vec!["glm-5.2", "glm-4.5", "glm-4.5-air", "glm-4.5-flash", "glm-4.5v"],
        ),
        ("fireworks-ai", vec!["accounts/glm-5p2", "accounts/kimi-k3"]),
        (
            "together",
            vec![
                "deepseek-ai/DeepSeek-R1",
                "MiniMaxAI/MiniMax-M2.7",
                "openai/gpt-oss-20b",
                "openai/gpt-oss-120b",
                "deepseek-ai/DeepSeek-V4-Pro",
            ],
        ),
        (
            "github-copilot",
            vec![
                "claude-sonnet-4.5",
                "claude-opus-4.7",
                "gpt-5.4",
                "grok-4.5",
                "oswe-model",
                "mai-model",
            ],
        ),
        (
            "kimi-for-coding",
            vec![
                "k2p5",
                "k2p6",
                "k2p7",
                "kimi-for-coding",
                "k3",
                "kimi-for-coding-highspeed",
                "kimi-k2-thinking",
            ],
        ),
        ("moonshotai", vec!["kimi-k3"]),
        ("moonshotai-cn", vec!["kimi-k3"]),
        (
            "alibaba-token-plan",
            vec![
                "deepseek-v4-flash-0731",
                "deepseek-v4-pro",
                "glm-5.2",
                "qwen3.6-flash",
                "qwen3.7-max",
                "qwen3.7-plus",
                "qwen3.8-max",
                "qwen3.8-max-preview",
                "MiniMax-M2.5",
                "qwen3.5-plus",
            ],
        ),
    ] {
        for id in ids {
            data[provider]["models"][id] = json!({"tool_call":true,"reasoning":true});
        }
    }
    for provider in ["opencode", "opencode-go"] {
        for (id, npm) in [
            ("responses", "@ai-sdk/openai"),
            ("anthropic", "@ai-sdk/anthropic"),
            ("google", "@ai-sdk/google"),
            ("alibaba", "@ai-sdk/alibaba"),
            ("grok-build-0.1", "@ai-sdk/openai-compatible"),
            ("kimi-k2.6", "@ai-sdk/anthropic"),
            ("minimax-m2.7", "@ai-sdk/anthropic"),
            ("qwen3.5-plus", "@ai-sdk/anthropic"),
            ("qwen3.6-plus", "@ai-sdk/anthropic"),
            ("deepseek-v4-pro", "@ai-sdk/openai-compatible"),
        ] {
            data[provider]["models"][id] = json!({"tool_call":true,"provider":{"npm":npm}});
        }
    }
    data
}

#[test]
fn source_provider_alias_pricing_and_lossless_nvidia_boundaries_match() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    for body in [
        r#"{"mistral":{"models":{"trailing":{"tool_call":true,"cost":{"input":"1bad"}},"hex":{"tool_call":true,"cost":{"input":"0x10"}},"false-cache":{"tool_call":true,"cost":{"cache_read":false}}}}}"#,
        r#"{"nvidia":{"models":{"a\ud800_x":{"tool_call":true,"modalities":{"input":["text"],"output":["text"]}},"a\ud801_x":{"tool_call":true,"modalities":{"input":["text"],"output":["text"]}}}}}"#,
        r#"{"together":null,"togetherai":{"models":{"alias":{"tool_call":true,"reasoning":true}}}}"#,
        r#"{"together":{"models":{}},"togetherai":{"models":{"ignored":{"tool_call":true}}}}"#,
        r#"{"together-ai":{"models":{"alias":{"tool_call":true}}}}"#,
    ] {
        let nvidia = r#"{"data":[{"id":"a\ud800.x"},{"id":"a\ud801.x"}]}"#;
        let expected=source.request(&js_json!({"operation":"loadModelsDevData","responses":{"https://models.dev/api.json":{"bodyText":body},"https://integrate.api.nvidia.com/v1/models":{"bodyText":nvidia}}}));
        let mut fetch = |url: &str| {
            Ok(HttpResponse {
                status: 200,
                body: if url == "https://models.dev/api.json" {
                    body.to_owned()
                } else {
                    nvidia.to_owned()
                },
            })
        };
        let mut generator = Generator::new(&mut fetch);
        assert_eq!(
            stringify_lossless(&generator.load_models_dev_data().unwrap()),
            stringify_lossless(&expected["result"]),
            "{body}"
        );
        assert_eq!(
            stringify_lossless(&pi_js_compat::js_json::JsValue::Array(generator.logs)),
            stringify_lossless(&expected["logs"])
        );
    }
}

#[test]
fn source_models_dev_every_provider_pipeline_matches_in_source_order() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let body = serde_json::to_string(&fixture()).unwrap();
    let nvidia = r#"{"data":[{"id":"plain"},{"id":"deprecated"}]}"#;
    let expected=source.request(&js_json!({"operation":"loadModelsDevData","responses":{"https://models.dev/api.json":{"bodyText":body},"https://integrate.api.nvidia.com/v1/models":{"bodyText":nvidia}}}));
    let mut requests = vec![];
    let mut fetch = |url: &str| {
        requests.push(url.to_owned());
        Ok(HttpResponse {
            status: 200,
            body: if url == "https://models.dev/api.json" {
                body.clone()
            } else {
                nvidia.to_owned()
            },
        })
    };
    let mut generator = Generator::new(&mut fetch);
    let actual = generator.load_models_dev_data().unwrap();
    let expected_models = expected["result"].clone();
    let (pi_js_compat::js_json::JsValue::Array(actual), pi_js_compat::js_json::JsValue::Array(expected_models)) =
        (&actual, &expected_models)
    else {
        panic!("model pipeline must return arrays");
    };
    assert_eq!(actual.len(), expected_models.len());
    for (index, (actual, expected)) in actual.iter().zip(expected_models).enumerate() {
        assert_eq!(
            stringify_lossless(actual),
            stringify_lossless(expected),
            "model {index}"
        );
    }
    assert_eq!(
        requests,
        vec![
            "https://models.dev/api.json",
            "https://integrate.api.nvidia.com/v1/models"
        ]
    );
}
