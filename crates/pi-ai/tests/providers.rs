//! Behavioral tests for `packages/ai/src/providers/*.ts` and `providers/all.ts`.

use std::sync::Arc;

use pi_ai::images_models::ImagesModels;
use pi_ai::models::{CreateModelsOptions, Models, MutableModels, Provider, create_models};
use pi_ai::providers::all::{
    builtin_images_models, builtin_images_providers, builtin_models, builtin_providers, get_builtin_model,
    get_builtin_model_data_generated_at, get_builtin_models, get_builtin_providers,
};
use pi_ai::request::{SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model, ModelCompat, ModelCostRates};
use serde_json::json;

#[allow(dead_code)]
#[path = "support/recording_fetch.rs"]
mod recording_fetch;

use recording_fetch::{RecordingFetch, ScriptedResponse};

fn provider(id: &str) -> Arc<dyn Provider> {
    builtin_providers()
        .into_iter()
        .find(|provider| provider.id() == id)
        .unwrap_or_else(|| panic!("missing provider {id}"))
}

fn api_key_name(provider: &dyn Provider) -> String {
    provider
        .auth()
        .api_key
        .as_ref()
        .expect("api key auth")
        .name()
        .to_owned()
}

fn oauth_name(provider: &dyn Provider) -> Option<String> {
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
    let expected = [
        Expected {
            id: "amazon-bedrock",
            name: "Amazon Bedrock",
            base_url: None,
            api_key_auth: "AWS credentials or bearer token",
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
            id: "baseten",
            name: "Baseten",
            base_url: Some("https://inference.baseten.co/v1"),
            api_key_auth: "Baseten API key",
            oauth: None,
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
            oauth: Some("Kimi Code (subscription)"),
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
            name: "OpenCode Go",
            base_url: None,
            api_key_auth: "OpenCode API key",
            oauth: None,
        },
        Expected {
            id: "openrouter",
            name: "OpenRouter",
            base_url: Some("https://openrouter.ai/api/v1"),
            api_key_auth: "OpenRouter API key",
            oauth: Some("OpenRouter OAuth"),
        },
        Expected {
            id: "qwen-token-plan",
            name: "Qwen Token Plan",
            base_url: Some("https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"),
            api_key_auth: "Qwen Token Plan API key",
            oauth: None,
        },
        Expected {
            id: "qwen-token-plan-cn",
            name: "Qwen Token Plan CN",
            base_url: Some("https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"),
            api_key_auth: "Qwen Token Plan CN API key",
            oauth: None,
        },
        Expected {
            id: "qwen-token-plan-individual",
            name: "Qwen Token Plan Individual",
            base_url: Some("https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1"),
            api_key_auth: "Qwen Token Plan Individual API key",
            oauth: None,
        },
        Expected {
            id: "radius",
            name: "Radius",
            base_url: None,
            api_key_auth: "Radius API key",
            oauth: Some("Radius"),
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
            oauth: Some("xAI (Grok/X subscription)"),
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
        assert_eq!(provider.base_url(), entry.base_url, "{} baseUrl", entry.id);
        assert_eq!(api_key_name(provider.as_ref()), entry.api_key_auth, "{} auth", entry.id);
        assert_eq!(
            oauth_name(provider.as_ref()).as_deref(),
            entry.oauth,
            "{} oauth",
            entry.id
        );
        assert!(provider.headers().is_none(), "{} headers", entry.id);
    }
}

#[test]
fn oauth_subscription_flags_match_the_ts_factories() {
    let flags: Vec<(String, bool, Option<String>)> = builtin_providers()
        .iter()
        .filter_map(|provider| {
            let oauth = provider.auth().oauth.as_ref()?;
            Some((
                provider.id().to_owned(),
                oauth.is_subscription(),
                oauth.login_label().map(str::to_owned),
            ))
        })
        .collect();
    assert_eq!(
        flags,
        vec![
            ("anthropic".to_owned(), true, None),
            ("github-copilot".to_owned(), true, None),
            (
                "kimi-coding".to_owned(),
                true,
                Some("Sign in with Kimi Code".to_owned())
            ),
            ("openai-codex".to_owned(), true, None),
            ("openai-codex-gc".to_owned(), true, None),
            (
                "openrouter".to_owned(),
                false,
                Some("Sign in with OpenRouter".to_owned())
            ),
            ("radius".to_owned(), false, None),
            (
                "xai".to_owned(),
                true,
                Some("Sign in with SuperGrok or X Premium".to_owned())
            ),
        ]
    );
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
        if provider.id() == "radius" {
            assert!(list.is_empty(), "radius has no static catalog");
        } else {
            assert!(!list.is_empty(), "{} has models", provider.id());
        }
        assert!(list.iter().all(|model| model.provider == provider.id()));
    }
}

#[test]
fn catalog_keeps_ts_generated_order_and_metadata() {
    let anthropic = get_builtin_models("anthropic");
    assert_eq!(anthropic.len(), 19);
    assert_eq!(anthropic[0].id, "claude-fable-5");

    let openai = get_builtin_model("openai", "gpt-4o").expect("gpt-4o");
    let ModelCompat::OpenAIResponses(compat) = openai.compat.expect("compat") else {
        panic!("openai-responses compat");
    };
    assert_eq!(compat.supports_strict_mode, Some(true));
    assert_eq!(compat.supports_openai_grammar_tools, None);

    let gpt = get_builtin_model("openai", "gpt-5.4").expect("gpt-5.4");
    let ModelCompat::OpenAIResponses(compat) = gpt.compat.expect("compat") else {
        panic!("openai-responses compat");
    };
    assert_eq!(compat.supports_strict_mode, Some(true));
    assert_eq!(compat.supports_openai_grammar_tools, Some(true));

    let haiku = get_builtin_model("anthropic", "claude-haiku-4-5").expect("haiku");
    let ModelCompat::AnthropicMessages(compat) = haiku.compat.expect("compat") else {
        panic!("anthropic compat");
    };
    assert_eq!(compat.supports_strict_tools, Some(true));

    // `providers.test.ts` still expects `cacheWrite: 0`; the pinned generated
    // catalog carries 3 (recorded source discrepancy in the traceability doc).
    for provider_id in ["moonshotai", "moonshotai-cn"] {
        let kimi = get_builtin_model(provider_id, "kimi-k3").expect("kimi-k3");
        assert_eq!(
            kimi.cost.rates,
            ModelCostRates {
                input: 3.0,
                output: 15.0,
                cache_read: 0.3,
                cache_write: 3.0,
            }
        );
    }

    let coding_k3 = get_builtin_model("kimi-coding", "k3").expect("k3");
    assert_eq!(coding_k3.cost.rates.input, 3.0);
    let highspeed = get_builtin_model("kimi-coding", "kimi-for-coding-highspeed").expect("highspeed");
    assert_eq!(
        highspeed.cost.rates,
        ModelCostRates {
            input: 1.9,
            output: 8.0,
            cache_read: 0.38,
            cache_write: 0.0,
        }
    );

    // `together-models.test.ts` still names `moonshotai/Kimi-K2.6`; the pinned
    // catalog ships Kimi-K3 instead (recorded source discrepancy).
    let together = get_builtin_model("together", "moonshotai/Kimi-K3").expect("together kimi");
    assert_eq!(together.base_url, "https://api.together.ai/v1");
    assert!(together.reasoning);
    assert_eq!(together.context_window, 1048576.0);
    assert_eq!(together.max_tokens, 131072.0);

    assert!(get_builtin_model("baseten", "zai-org/GLM-5.2").is_none());

    let bedrock_ids: Vec<String> = get_builtin_models("amazon-bedrock")
        .into_iter()
        .map(|model| model.id)
        .collect();
    assert!(bedrock_ids.contains(&"global.anthropic.claude-opus-5".to_owned()));
    assert!(!bedrock_ids.contains(&"anthropic.claude-opus-5".to_owned()));

    for model_id in ["mimo-v2-flash", "mimo-v2-omni"] {
        assert!(get_builtin_model("xiaomi", model_id).is_some(), "xiaomi {model_id}");
        for provider_id in ["xiaomi-token-plan-cn", "xiaomi-token-plan-ams", "xiaomi-token-plan-sgp"] {
            assert!(
                get_builtin_model(provider_id, model_id).is_none(),
                "{provider_id} must omit {model_id}"
            );
        }
    }
}

#[test]
fn staged_timestamp_helper_has_no_fabricated_source_timestamp() {
    // Kept only while coding-agent model_runtime still consumes the old API.
    assert_eq!(get_builtin_model_data_generated_at(), None);
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
        let mut apis: Vec<String> = provider.get_models().into_iter().map(|model| model.api).collect();
        apis.sort();
        apis.dedup();
        for api in apis {
            let model: Model = provider
                .get_models()
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
                            CODEX_TEST_TOKEN.to_owned()
                        } else {
                            "test-key".to_owned()
                        }),
                        fetch: Some(fetch.clone() as Arc<dyn pi_ai::request::Fetch>),
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
                .result()
                .await
                .expect("stream result");
            let unimplemented = provider.id() == "opencode-go" && api == "google-generative-ai";
            let message = result.error_message.unwrap_or_default();
            if unimplemented {
                assert!(
                    message.contains("has no API implementation"),
                    "opencode-go ({api}) source behavior: {message}"
                );
                assert!(fetch.requests().is_empty());
                continue;
            }
            assert!(
                !message.contains("no API implementation"),
                "{} ({api}): {message}",
                provider.id()
            );
            assert_eq!(
                fetch.requests().len(),
                1,
                "{} ({api}) must issue one request",
                provider.id()
            );
        }
    }
}

#[test]
fn azure_factory_is_available_but_not_built_in() {
    let provider = pi_ai::providers::azure_openai_responses::azure_openai_responses_provider();
    assert_eq!(provider.id(), "azure-openai-responses");
    assert_eq!(provider.name(), "Azure OpenAI");
    assert!(provider.base_url().is_none());
    assert!(!provider.get_models().is_empty());

    let models = create_models(CreateModelsOptions::default());
    models.set_provider(provider);
    assert!(!models.get_models(Some("azure-openai-responses")).is_empty());
}
