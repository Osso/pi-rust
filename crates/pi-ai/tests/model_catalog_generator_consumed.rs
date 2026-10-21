use pi_ai::model_catalog_generator::{
    Generator, GeneratorError, GeneratorOptions, HttpResponse, get_models_dev_cost, process_baseten_models,
    process_fireworks_models,
};
use pi_js_compat::js_json::{JsValue, parse_lossless, stringify_lossless};
use serde_json::{Value, json};

fn assert_type_error(result: Result<JsValue, GeneratorError>, message: &str) {
    let error = result.expect_err(message);
    assert_eq!(error.name, "TypeError");
    assert_eq!(error.message.to_utf8().unwrap(), message);
    assert!(error.details.is_none());
}

fn load(body: &Value, strict: bool) -> Result<JsValue, GeneratorError> {
    // Strict loading always checks the Individual allowlist, even when absent upstream.
    let required = [
        "deepseek-v4-flash-0731",
        "deepseek-v4-pro",
        "glm-5.2",
        "qwen3.6-flash",
        "qwen3.7-max",
        "qwen3.7-plus",
        "qwen3.8-max",
    ];
    let mut body = body.clone();
    if body["alibaba-token-plan"].is_null() {
        body["alibaba-token-plan"] = json!({"models":{}});
    }
    for id in required {
        body["alibaba-token-plan"]["models"][id] = json!({"tool_call":true});
    }
    let mut fetch = |url: &str| {
        Ok(HttpResponse {
            status: 200,
            body: if url.contains("nvidia.com") {
                json!({"data":[{"id":"one"}]}).to_string()
            } else {
                body.to_string()
            },
        })
    };
    let mut models = Generator::new(
        GeneratorOptions {
            strict,
            ..Default::default()
        },
        &mut fetch,
    )
    .load_models_dev_data()?;
    if let JsValue::Array(models) = &mut models {
        models.retain(|model| {
            let value: Value = serde_json::from_str(&stringify_lossless(model)).unwrap();
            !required.iter().any(|id| value["id"] == *id)
        });
    }
    Ok(models)
}

#[test]
fn consumed_reasoning_rejects_strings_in_every_provider_family() {
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
        "togetherai",
        "together-ai",
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
        let id = match provider {
            "google-vertex" => "gemini-one",
            "cloudflare-ai-gateway" => "openai/one",
            _ => "one",
        };
        let body = json!({provider:{"models":{id:{"tool_call":true,"reasoning":"yes","modalities":{"input":["text"],"output":["text"]}}}}});
        assert_type_error(load(&body, true), "models.dev reasoning must be a boolean");
        assert_eq!(stringify_lossless(&load(&body, false).unwrap()), "[]", "{provider}");
    }
}

#[test]
fn consumed_filter_and_routing_fields_reject_wrong_types_at_use() {
    for (provider, extra, message) in [
        (
            "openai",
            json!({"tool_call":"yes"}),
            "models.dev tool_call must be a boolean",
        ),
        (
            "fireworks-ai",
            json!({"tool_call":1}),
            "models.dev tool_call must be a boolean",
        ),
        ("baseten", json!({"status":false}), "models.dev status must be a string"),
        ("together", json!({"status":[]}), "models.dev status must be a string"),
        ("opencode", json!({"status":1}), "models.dev status must be a string"),
        (
            "github-copilot",
            json!({"status":{}}),
            "models.dev status must be a string",
        ),
        (
            "amazon-bedrock",
            json!({"structured_output":"yes"}),
            "models.dev structured_output must be a boolean",
        ),
        (
            "opencode",
            json!({"provider":false}),
            "models.dev model.provider must be an object",
        ),
        (
            "opencode-go",
            json!({"provider":{"npm":1}}),
            "models.dev provider.npm must be a string",
        ),
    ] {
        let mut model = json!({"tool_call":true});
        model
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_type_error(load(&json!({provider:{"models":{"one":model}}}), true), message);
    }
    assert_type_error(
        process_baseten_models(&JsValue::from(&json!({"models":{"one":{"reasoning":"yes"}}}))),
        "models.dev reasoning must be a boolean",
    );
    assert_type_error(
        process_fireworks_models(&JsValue::from(&json!({"models":{"one":{"tool_call":"yes"}}}))),
        "models.dev tool_call must be a boolean",
    );
    assert_type_error(
        get_models_dev_cost(&JsValue::from(&json!({"tiers":[{"tier":{"type":3}}]}))),
        "models.dev cost.tiers.tier.type must be a string",
    );
}

#[test]
fn nvidia_validates_only_modalities_reached_by_the_text_filter() {
    for (modalities, message) in [
        (
            json!({"input":"text"}),
            "models.dev modalities.input must be an array of strings",
        ),
        (
            json!({"input":["text"],"output":[3]}),
            "models.dev modalities.output must be an array of strings",
        ),
    ] {
        let body = json!({"nvidia":{"models":{"one":{"tool_call":true,"modalities":modalities}}}});
        assert_type_error(load(&body, true), message);
    }
    let skipped = json!({"nvidia":{"models":{"one":{"tool_call":true,"modalities":{"input":["audio"],"output":3},"reasoning":"unused"}}}});
    assert_eq!(stringify_lossless(&load(&skipped, true).unwrap()), "[]");
}

#[test]
fn openrouter_consumed_collections_and_receivers_reject_wrong_types() {
    for (extra, message) in [
        (
            json!({"supported_parameters":["tools",3]}),
            "OpenRouter supported_parameters entries must be strings",
        ),
        (
            json!({"architecture":false}),
            "OpenRouter architecture must be an object",
        ),
        (
            json!({"architecture":{"modality":["image",3]}}),
            "OpenRouter modality entries must be strings",
        ),
        (json!({"pricing":[]}), "OpenRouter pricing must be an object"),
        (
            json!({"top_provider":false}),
            "OpenRouter top_provider must be an object",
        ),
    ] {
        let mut model = json!({"id":"one","name":"One","supported_parameters":["tools"]});
        model
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let body = json!({"data":[model]}).to_string();
        for strict in [false, true] {
            let mut fetch = |_: &str| {
                Ok(HttpResponse {
                    status: 200,
                    body: body.clone(),
                })
            };
            let mut generator = Generator::new(
                GeneratorOptions {
                    strict,
                    ..Default::default()
                },
                &mut fetch,
            );
            let result = generator.fetch_open_router_models();
            if strict {
                assert_type_error(result, message);
            } else {
                assert_eq!(stringify_lossless(&result.unwrap()), "[]");
            }
            assert_eq!(generator.logs.len(), 2);
            assert_eq!(
                stringify_lossless(&generator.logs[1]),
                json!([
                    "error",
                    "Failed to fetch OpenRouter models:",
                    format!("TypeError: {message}")
                ])
                .to_string()
            );
        }
    }
}

#[test]
fn ignored_fields_unknown_controls_and_short_circuits_remain_accepted() {
    for body in [
        json!({"openai":{"models":{"one":{"tool_call":false,"reasoning":"unused","modalities":3,"status":false,"provider":3}}}}),
        json!({"baseten":{"models":{"one":{"status":"deprecated","reasoning":"unused","reasoning_options":3}}}}),
        json!({"amazon-bedrock":{"models":{"ai21.jamba-one":{"tool_call":true,"structured_output":"unused","reasoning":"unused"}}}}),
        json!({"opencode":{"models":{"one":{"tool_call":true,"provider":{"npm":"unknown-package"},"reasoning_options":[{"type":"unknown-kind","values":false}]}}}}),
        json!({"kimi-for-coding":{"models":{"k3":{"tool_call":true,"reasoning":"unused"}}}}),
        json!({"moonshotai":{"models":{"kimi-k3":{"tool_call":true,"reasoning":"unused"}}}}),
        json!({"google":{"models":{"gemini-flash-latest":{"tool_call":true,"reasoning":"unused","cost":false,"modalities":false},"gemini-3.5-flash":{"name":false,"reasoning":true}}}}),
        json!({"google-vertex":{"models":{"gemini-2.5-flash":{"tool_call":true,"cost":{"cache_read":false,"cache_write":{}}}}}}),
        json!({"baseten":{"models":{"one":{"id":3,"structured_output":3,"tool_call":3,"provider":3,"modalities":{"output":3},"unknown":{},"reasoning_options":[{"type":"future","values":false},{"type":"budget_tokens","min":{},"max":false},{"type":"effort","values":["unknown","high",null]}]}}}}),
    ] {
        load(&body, true).unwrap_or_else(|error| panic!("{body}: {error}"));
    }
    let cost = parse_lossless(
        r#"{"tiers":[{"tier":{"type":"unknown","size":false},"input":{}},{"tier":{"type":"context"},"input":{}}]}"#,
    )
    .unwrap();
    get_models_dev_cost(&cost).unwrap();
}

#[test]
fn copied_scalars_keep_the_approved_emission_boundary_and_defaults() {
    let body = json!({"openai":{"models":{"one":{"tool_call":true,"name":false,"cost":{"input":false},"limit":{"context":false}}}}});
    let models = load(&body, true).unwrap();
    let values: Value = serde_json::from_str(&stringify_lossless(&models)).unwrap();
    assert_eq!(values[0]["name"], "one");
    assert_eq!(values[0]["cost"]["input"], 0);
    assert_eq!(values[0]["contextWindow"], 4096);
    let copied = json!({"openai":{"models":{"one":{"tool_call":true,"name":3,"cost":{"input":"copied"},"limit":{"output":"copied"}}}}});
    let values: Value = serde_json::from_str(&stringify_lossless(&load(&copied, true).unwrap())).unwrap();
    assert_eq!(values[0]["name"], 3);
    assert_eq!(values[0]["cost"]["input"], "copied");
    assert_eq!(values[0]["maxTokens"], "copied");
}
