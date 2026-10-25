//! Behavioral tests for `packages/ai/src/providers/*.ts` and `providers/all.ts`.

use std::sync::Arc;

use pi_ai::images_models::ImagesModels;
use pi_ai::models::{CreateModelsOptions, Models, Provider};
use pi_ai::providers::all::{
    builtin_images_models, builtin_images_providers, builtin_models, builtin_providers, get_builtin_model,
    get_builtin_providers,
};
use pi_ai::request::{SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model};
use serde_json::json;

#[allow(dead_code)]
use crate::shared_recording_fetch as recording_fetch;

use recording_fetch::{RecordingFetch, ScriptedResponse};

fn provider(id: &str) -> Arc<dyn Provider> {
    builtin_providers()
        .into_iter()
        .find(|provider| provider.id() == id)
        .unwrap_or_else(|| panic!("missing provider {id}"))
}

fn api_key_name(provider: &dyn Provider) -> pi_js_compat::js_json::JsString {
    provider
        .auth()
        .api_key
        .as_ref()
        .expect("api key auth")
        .name()
        .to_owned()
}

fn oauth_name(provider: &dyn Provider) -> Option<pi_js_compat::js_json::JsString> {
    provider.auth().oauth.as_ref().map(|oauth| oauth.name().to_owned())
}

#[test]
fn builtin_provider_order_matches_all_ts() {
    let ids: Vec<String> = builtin_providers()
        .iter()
        .map(|provider| provider.id().to_owned())
        .collect();
    assert_eq!(
        ids,
        vec![
            "amazon-bedrock",
            "ant-ling",
            "anthropic",
            "cerebras",
            "cloudflare-ai-gateway",
            "cloudflare-workers-ai",
            "deepseek",
            "fireworks",
            "github-copilot",
            "google",
            "google-vertex",
            "groq",
            "huggingface",
            "kimi-coding",
            "minimax",
            "minimax-cn",
            "mistral",
            "moonshotai",
            "moonshotai-cn",
            "nvidia",
            "openai",
            "openai-codex",
            "openai-codex-gc",
            "openai-codex-team",
            "opencode",
            "opencode-go",
            "openrouter",
            "together",
            "vercel-ai-gateway",
            "xai",
            "xiaomi",
            "xiaomi-token-plan-ams",
            "xiaomi-token-plan-cn",
            "xiaomi-token-plan-sgp",
            "zai",
            "zai-coding-cn",
        ]
    );
    assert!(!get_builtin_providers().iter().any(|id| id == "azure-openai-responses"));
}

#[test]
fn provider_metadata_matches_the_ts_factories() {
    struct Expected {
        id: &'static str,
        name: &'static str,
        base_url: Option<&'static str>,
        api_key_auth: &'static str,
        oauth: Option<&'static str>,
    }
    // Frozen providers/*.ts at 4f9e6f8a4; OAuth-only Codex factories are
    // covered by the inventory and OAuth fixtures below.
    let expected = [
        Expected {
            id: "amazon-bedrock",
            name: "Amazon Bedrock",
            base_url: None,
            api_key_auth: "AWS credentials",
            oauth: None,
        },
        Expected {
            id: "ant-ling",
            name: "Ant Ling",
            base_url: Some("https://api.ant-ling.com/v1"),
            api_key_auth: "Ant Ling API key",
            oauth: None,
        },
        Expected {
            id: "anthropic",
            name: "Anthropic",
            base_url: Some("https://api.anthropic.com"),
            api_key_auth: "Anthropic API key",
            oauth: Some("Anthropic (Claude Pro/Max)"),
        },
        Expected {
            id: "cerebras",
            name: "Cerebras",
            base_url: Some("https://api.cerebras.ai/v1"),
            api_key_auth: "Cerebras API key",
            oauth: None,
        },
        Expected {
            id: "cloudflare-ai-gateway",
            name: "Cloudflare AI Gateway",
            base_url: None,
            api_key_auth: "Cloudflare API key",
            oauth: None,
        },
        Expected {
            id: "cloudflare-workers-ai",
            name: "Cloudflare Workers AI",
            base_url: None,
            api_key_auth: "Cloudflare API key",
            oauth: None,
        },
        Expected {
            id: "deepseek",
            name: "DeepSeek",
            base_url: Some("https://api.deepseek.com"),
            api_key_auth: "DeepSeek API key",
            oauth: None,
        },
        Expected {
            id: "fireworks",
            name: "Fireworks",
            base_url: Some("https://api.fireworks.ai/inference"),
            api_key_auth: "Fireworks API key",
            oauth: None,
        },
        Expected {
            id: "github-copilot",
            name: "GitHub Copilot",
            base_url: Some("https://api.individual.githubcopilot.com"),
            api_key_auth: "GitHub Copilot token",
            oauth: Some("GitHub Copilot"),
        },
        Expected {
            id: "google",
            name: "Google",
            base_url: Some("https://generativelanguage.googleapis.com/v1beta"),
            api_key_auth: "Gemini API key",
            oauth: None,
        },
        Expected {
            id: "google-vertex",
            name: "Google Vertex AI",
            base_url: None,
            api_key_auth: "Google Cloud credentials",
            oauth: None,
        },
        Expected {
            id: "groq",
            name: "Groq",
            base_url: Some("https://api.groq.com/openai/v1"),
            api_key_auth: "Groq API key",
            oauth: None,
        },
        Expected {
            id: "huggingface",
            name: "Hugging Face",
            base_url: Some("https://router.huggingface.co/v1"),
            api_key_auth: "Hugging Face token",
            oauth: None,
        },
        Expected {
            id: "kimi-coding",
            name: "Kimi For Coding",
            base_url: Some("https://api.kimi.com/coding"),
            api_key_auth: "Kimi API key",
            oauth: None,
        },
        Expected {
            id: "minimax",
            name: "MiniMax",
            base_url: Some("https://api.minimax.io/anthropic"),
            api_key_auth: "MiniMax API key",
            oauth: None,
        },
        Expected {
            id: "minimax-cn",
            name: "MiniMax CN",
            base_url: Some("https://api.minimaxi.com/anthropic"),
            api_key_auth: "MiniMax CN API key",
            oauth: None,
        },
        Expected {
            id: "mistral",
            name: "Mistral",
            base_url: Some("https://api.mistral.ai"),
            api_key_auth: "Mistral API key",
            oauth: None,
        },
        Expected {
            id: "moonshotai",
            name: "Moonshot AI",
            base_url: Some("https://api.moonshot.ai/v1"),
            api_key_auth: "Moonshot AI API key",
            oauth: None,
        },
        Expected {
            id: "moonshotai-cn",
            name: "Moonshot AI CN",
            base_url: Some("https://api.moonshot.cn/v1"),
            api_key_auth: "Moonshot AI API key",
            oauth: None,
        },
        Expected {
            id: "nvidia",
            name: "NVIDIA",
            base_url: Some("https://integrate.api.nvidia.com/v1"),
            api_key_auth: "NVIDIA API key",
            oauth: None,
        },
        Expected {
            id: "openai",
            name: "OpenAI",
            base_url: Some("https://api.openai.com/v1"),
            api_key_auth: "OpenAI API key",
            oauth: None,
        },
        Expected {
            id: "opencode",
            name: "OpenCode Zen",
            base_url: None,
            api_key_auth: "OpenCode API key",
            oauth: None,
        },
        Expected {
            id: "opencode-go",
            name: "OpenCode Zen Go",
            base_url: None,
            api_key_auth: "OpenCode API key",
            oauth: None,
        },
        Expected {
            id: "openrouter",
            name: "OpenRouter",
            base_url: Some("https://openrouter.ai/api/v1"),
            api_key_auth: "OpenRouter API key",
            oauth: None,
        },
        Expected {
            id: "together",
            name: "Together",
            base_url: Some("https://api.together.ai/v1"),
            api_key_auth: "Together API key",
            oauth: None,
        },
        Expected {
            id: "vercel-ai-gateway",
            name: "Vercel AI Gateway",
            base_url: Some("https://ai-gateway.vercel.sh"),
            api_key_auth: "Vercel AI Gateway API key",
            oauth: None,
        },
        Expected {
            id: "xai",
            name: "xAI",
            base_url: Some("https://api.x.ai/v1"),
            api_key_auth: "xAI API key",
            oauth: None,
        },
        Expected {
            id: "xiaomi",
            name: "Xiaomi",
            base_url: Some("https://api.xiaomimimo.com/v1"),
            api_key_auth: "Xiaomi API key",
            oauth: None,
        },
        Expected {
            id: "xiaomi-token-plan-ams",
            name: "Xiaomi Token Plan AMS",
            base_url: Some("https://token-plan-ams.xiaomimimo.com/v1"),
            api_key_auth: "Xiaomi Token Plan AMS API key",
            oauth: None,
        },
        Expected {
            id: "xiaomi-token-plan-cn",
            name: "Xiaomi Token Plan CN",
            base_url: Some("https://token-plan-cn.xiaomimimo.com/v1"),
            api_key_auth: "Xiaomi Token Plan CN API key",
            oauth: None,
        },
        Expected {
            id: "xiaomi-token-plan-sgp",
            name: "Xiaomi Token Plan SGP",
            base_url: Some("https://token-plan-sgp.xiaomimimo.com/v1"),
            api_key_auth: "Xiaomi Token Plan SGP API key",
            oauth: None,
        },
        Expected {
            id: "zai",
            name: "Z.AI",
            base_url: Some("https://api.z.ai/api/coding/paas/v4"),
            api_key_auth: "Z.AI API key",
            oauth: None,
        },
        Expected {
            id: "zai-coding-cn",
            name: "Z.AI Coding CN",
            base_url: Some("https://open.bigmodel.cn/api/coding/paas/v4"),
            api_key_auth: "Z.AI Coding CN API key",
            oauth: None,
        },
    ];

    for entry in expected {
        let provider = provider(entry.id);
        assert_eq!(provider.id(), entry.id);
        assert_eq!(provider.name(), entry.name, "{} name", entry.id);
        assert_eq!(
            provider.base_url(),
            entry.base_url.map(pi_js_compat::js_json::JsString::from).as_ref(),
            "{} baseUrl",
            entry.id
        );
        assert_eq!(api_key_name(provider.as_ref()), entry.api_key_auth, "{} auth", entry.id);
        assert_eq!(
            oauth_name(provider.as_ref()),
            entry.oauth.map(pi_js_compat::js_json::JsString::from),
            "{} oauth",
            entry.id
        );
        assert!(provider.headers().is_none(), "{} headers", entry.id);
    }
}

#[test]
fn builtin_models_registers_every_builtin_provider_with_models() {
    let models = builtin_models(CreateModelsOptions::default());
    let providers = models.get_providers();
    assert_eq!(providers.len(), builtin_providers().len());
    assert!(providers.iter().any(|provider| provider.id() == "anthropic"));
    assert!(
        !providers
            .iter()
            .any(|provider| provider.id() == "azure-openai-responses")
    );

    assert_eq!(
        models.get_model("anthropic", "claude-haiku-4-5").map(|model| model.api),
        Some("anthropic-messages".to_owned())
    );
    assert!(models.get_models(None).len() > 500);

    for provider in providers {
        let list = models.get_models(Some(provider.id()));
        assert!(!list.is_empty(), "{} has models", provider.id());
        assert!(list.iter().all(|model| model.provider == provider.id()));
    }
}

#[test]
fn catalog_readers_preserve_optional_metadata() {
    for (provider, id) in [
        ("openai", "gpt-4o"),
        ("openai", "gpt-5.4"),
        ("anthropic", "claude-haiku-4-5"),
    ] {
        let model = get_builtin_model(provider, id).expect("frozen model");
        assert!(model.compat.is_none(), "{provider}/{id} has no source compat");
    }

    let xiaomi = get_builtin_model("xiaomi", "mimo-v2-flash").expect("MiMo");
    assert_eq!(
        serde_json::to_value(xiaomi.compat).expect("serialize compat"),
        serde_json::json!({
            "requiresReasoningContentOnAssistantMessages": true,
            "thinkingFormat": "deepseek",
        })
    );

    let kimi = get_builtin_model("kimi-coding", "k2p7").expect("Kimi Coding");
    assert_eq!(
        kimi.headers.as_ref().and_then(|headers| headers.get("User-Agent")),
        Some(&pi_js_compat::js_json::JsString::from("KimiCLI/1.5"))
    );
}

#[test]
fn builtin_images_providers_and_models_register_openrouter() {
    let providers = builtin_images_providers();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].id(), "openrouter");

    let models = builtin_images_models(CreateModelsOptions::default());
    assert_eq!(models.get_providers().len(), 1);
    assert!(!models.get_models(Some("openrouter")).is_empty());
}

/// Codex models require a JWT access token; the payload carries the account id.
const CODEX_TEST_TOKEN: &str =
    "eyJhbGciOiJub25lIn0.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiYWNjdF90ZXN0In19.sig";

/// Every provider factory must dispatch models of each catalog API to a real
/// implementation, except the recorded `opencode-go` source discrepancy: its
/// catalog carries a `google-generative-ai` model the factory does not map.
#[tokio::test]
async fn every_catalog_api_dispatches_to_an_implementation() {
    for provider in builtin_providers() {
        let mut apis: Vec<String> = provider
            .get_models()
            .unwrap()
            .into_iter()
            .map(|model| model.api)
            .collect();
        apis.sort();
        apis.dedup();
        for api in apis {
            let model: Model = provider
                .get_models()
                .unwrap()
                .into_iter()
                .find(|model| model.api == api)
                .expect("model for api");
            let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
                400,
                &json!({"error": {"message": "recorded"}}).to_string(),
            )]);
            let codex = provider.id().starts_with("openai-codex");
            let options = SimpleStreamOptions {
                stream: StreamOptions {
                    request: pi_ai::request::ProviderRequestOptions {
                        api_key: Some(if codex {
                            (CODEX_TEST_TOKEN.to_owned()).into()
                        } else {
                            ("test-key".to_owned()).into()
                        }),
                        fetch: Some(fetch.clone() as Arc<dyn pi_ai::request::Fetch>),
                        // Vendor adapters require explicit region/project configuration.
                        env: Some(indexmap::IndexMap::from([
                            ("AWS_REGION".into(), "us-east-1".into()),
                            // Bedrock :153 uses bearerToken/env, not the generic apiKey.
                            ("AWS_BEARER_TOKEN_BEDROCK".into(), "test-key".into()),
                            ("GOOGLE_CLOUD_PROJECT".into(), "test-project".into()),
                            ("GOOGLE_CLOUD_LOCATION".into(), "us-central1".into()),
                        ])),
                        ..Default::default()
                    },
                    transport: codex.then_some(pi_ai::types::Transport::Sse),
                    ..Default::default()
                },
                ..Default::default()
            };
            let result = provider
                .stream_simple(
                    model.clone(),
                    Context {
                        system_prompt: None,
                        messages: Vec::new(),
                        tools: None,
                    },
                    options,
                )
                .expect("acquisition")
                .result()
                .await
                .expect("stream result");
            let unimplemented = provider.id() == "opencode-go" && api == "google-generative-ai";
            let message = result.error_message.unwrap_or_default();
            if unimplemented {
                assert!(
                    message.contains("has no API implementation"),
                    "opencode-go ({api}) source behavior: {message:?}"
                );
                assert!(fetch.requests().is_empty());
                continue;
            }
            assert!(
                !message.contains("no API implementation"),
                "{} ({api}): {message:?}",
                provider.id()
            );
            assert_eq!(
                fetch.requests().len(),
                1,
                "{} ({api}) must issue one request; result: {message:?}",
                provider.id()
            );
        }
    }
}
