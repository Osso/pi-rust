use crate::shared_oracle as oracle;
use pi_ai::model_catalog_generator::{Generator, HttpResponse};
use pi_js_compat::{
    js_json,
    js_json::{JsValue, parse_lossless, stringify_lossless},
};

fn transport_body_cases() -> Vec<JsValue> {
    let mut bodies = vec![
        JsValue::Null,
        js_json!({}),
        js_json!({"data":null}),
        js_json!({"data":true}),
        js_json!({"data":"abc"}),
        js_json!({"data":[null]}),
        js_json!({"data":[{}]}),
    ];
    let valid = js_json!({"id":"model","name":"name","supported_parameters":["tools","reasoning"],"tags":["tool-use","reasoning","vision"],"architecture":{"modality":"text+image->text"},"pricing":{"prompt":"1","completion":"2","input":"1","output":"2"}});
    for key in [
        "supported_parameters",
        "tags",
        "architecture",
        "pricing",
        "context_length",
        "context_window",
        "max_tokens",
        "top_provider",
        "id",
        "name",
    ] {
        for value in [
            JsValue::Null,
            js_json!(false),
            js_json!(17),
            js_json!("tools vision image"),
            js_json!([]),
            js_json!({}),
        ] {
            let mut model = valid.clone();
            model[key] = value;
            bodies.push(js_json!({"data":[model]}));
        }
    }
    for key in ["modality", "input_modalities"] {
        for value in [
            JsValue::Null,
            js_json!(false),
            js_json!(17),
            js_json!("image"),
            js_json!(["text", "image"]),
            js_json!({}),
        ] {
            let mut model = valid.clone();
            model["architecture"][key] = value;
            bodies.push(js_json!({"data":[model]}));
        }
    }
    for key in [
        "prompt",
        "completion",
        "input",
        "output",
        "input_cache_read",
        "input_cache_write",
    ] {
        for value in [
            JsValue::Null,
            js_json!(true),
            js_json!("1.5e-6suffix"),
            js_json!([]),
            js_json!({"toString":0}),
            js_json!({"a":1}),
        ] {
            let mut model = valid.clone();
            model["pricing"][key] = value;
            bodies.push(js_json!({"data":[model]}));
        }
    }
    bodies
}
fn models_dev_cases() -> Vec<JsValue> {
    let mut bodies = vec![
        JsValue::Null,
        js_json!({}),
        js_json!({"anthropic":{"models":[null]}}),
        js_json!({"anthropic":{"models":{"null-model":null}}}),
    ];
    let valid = js_json!({"name":"name","tool_call":true,"reasoning":true,"modalities":{"input":["text","image"],"output":["text"]},"cost":{"input":2},"limit":{"context":1024,"output":128}});
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
        "opencode",
        "opencode-go",
        "github-copilot",
        "minimax",
        "minimax-cn",
        "kimi-for-coding",
        "moonshotai",
        "moonshotai-cn",
        "xiaomi",
    ] {
        for key in [
            "tool_call",
            "reasoning",
            "modalities",
            "name",
            "cost",
            "limit",
            "provider",
            "status",
        ] {
            for value in [
                JsValue::Null,
                js_json!(false),
                js_json!(17),
                js_json!("image"),
                js_json!([]),
                js_json!({}),
            ] {
                let mut model = valid.clone();
                model[key] = value;
                let mut body = js_json!({});
                body[provider] = js_json!({"models":{"gemini-2.5-flash":model}});
                bodies.push(body);
            }
        }
        for parent in ["cost", "limit", "provider"] {
            let keys: &[&str] = match parent {
                "cost" => &["input", "output", "cache_read", "cache_write"],
                "limit" => &["context", "output"],
                _ => &["npm"],
            };
            for &key in keys {
                for value in [
                    JsValue::Null,
                    js_json!(false),
                    js_json!(17),
                    js_json!("0x10"),
                    js_json!("1bad"),
                    js_json!([]),
                    js_json!({"toString":0}),
                ] {
                    let mut model = valid.clone();
                    model[parent] = js_json!({key:value});
                    let mut body = js_json!({});
                    body[provider] = js_json!({"models":{"gemini-2.5-flash":model}});
                    bodies.push(body);
                }
            }
        }
        for value in [
            JsValue::Null,
            js_json!(false),
            js_json!(17),
            js_json!("image"),
            js_json!(["text", "image"]),
            js_json!({}),
        ] {
            let mut model = valid.clone();
            model["modalities"]["input"] = value;
            let mut body = js_json!({});
            body[provider] = js_json!({"models":{"gemini-2.5-flash":model}});
            bodies.push(body);
        }
    }
    bodies
}
// Frozen generate-models.ts:788–809 checks tool_call and modalities inside each
// model iteration. A later null model must not hide the first model's error.
#[test]
fn models_dev_reports_first_consumed_field_failure_before_later_null_model() {
    let body = js_json!({"anthropic":{"models":{
        "first":{"tool_call":true,"modalities":{"input":17}},
        "later":null
    }}});
    let responses = js_json!({"https://models.dev/api.json":{"body":body}});
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let expected = source.request(&js_json!({"operation":"loadModelsDevData","responses":responses}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut fetch = |url: &str| {
        Ok(HttpResponse {
            status: 200,
            body: stringify_lossless(&responses[url]["body"]),
        })
    };
    let mut native = Generator::new(&mut fetch);
    assert_eq!(native.load_models_dev_data().unwrap(), expected["result"]);
    assert_eq!(JsValue::Array(native.logs), expected["logs"]);
}

#[test]
fn external_json_wrong_types_follow_frozen_uses_defaults_coercions_and_catches() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let mut failures = Vec::new();
    for (operation, url, bodies) in [
        (
            "fetchOpenRouterModels",
            "https://openrouter.ai/api/v1/models",
            transport_body_cases(),
        ),
        (
            "fetchAiGatewayModels",
            "https://ai-gateway.vercel.sh/v1/models",
            transport_body_cases(),
        ),
        ("loadModelsDevData", "https://models.dev/api.json", models_dev_cases()),
        (
            "fetchNvidiaNimModelIds",
            "https://integrate.api.nvidia.com/v1/models",
            transport_body_cases(),
        ),
    ] {
        for body in bodies {
            let mut responses = js_json!({"https://integrate.api.nvidia.com/v1/models":{"body":{"data":[]}}});
            responses[url] = js_json!({"body":body});
            let expected = source.request(&js_json!({"operation":operation,"responses":responses}));
            assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
            let mut fetch = |url: &str| {
                Ok(HttpResponse {
                    status: 200,
                    body: stringify_lossless(&responses[url]["body"]),
                })
            };
            let mut native = Generator::new(&mut fetch);
            let actual = match operation {
                "fetchOpenRouterModels" => native.fetch_open_router_models(),
                "fetchAiGatewayModels" => native.fetch_ai_gateway_models(),
                "fetchNvidiaNimModelIds" => native.fetch_nvidia_nim_model_ids().map(|entries| {
                    JsValue::Array(
                        entries
                            .into_iter()
                            .map(|(key, value)| JsValue::Array(vec![JsValue::String(key), JsValue::String(value)]))
                            .collect(),
                    )
                }),
                _ => native.load_models_dev_data(),
            }
            .unwrap();
            let actual = parse_lossless(stringify_lossless(&actual)).unwrap();
            let logs = JsValue::Array(native.logs);
            if actual != expected["result"] || logs != expected["logs"] {
                failures.push(js_json!({"operation":operation,"body":body,"actual":actual,"expected":expected["result"],"actualLogs":logs,"expectedLogs":expected["logs"]}));
            }
        }
    }
    if !failures.is_empty() {
        let path = std::env::temp_dir().join(format!("pi-generator-ingress-diffs-{}.json", std::process::id()));
        std::fs::write(&path, stringify_lossless(&JsValue::Array(failures.clone()))).unwrap();
        eprintln!("All ingress differences: {}", path.display());
    }
    assert!(
        failures.is_empty(),
        "{} ingress cases differ; first={}",
        failures.len(),
        failures.first().map(stringify_lossless).unwrap_or_default()
    );
}
