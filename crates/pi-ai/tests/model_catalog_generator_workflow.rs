#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::{Generator, GeneratorOptions, HttpResponse};
use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
use serde_json::{Value, json};

fn dev_fixture() -> Value {
    let mut result = json!({});
    for (provider, ids) in [
        ("openai", vec!["gpt-5.4", "gpt-5.5", "gpt-5.6-terra", "gpt-5-pro"]),
        ("anthropic", vec!["claude-opus-4-6", "claude-sonnet-4-6"]),
        ("github-copilot", vec!["claude-opus-4.7"]),
        ("opencode", vec!["claude-sonnet-4", "gpt-5.4", "gpt-5.3-codex-spark"]),
        ("minimax", vec!["unsupported", "MiniMax-M2.7"]),
        ("kimi-for-coding", vec!["k2p5", "k2p6"]),
        ("xai", vec!["grok-3", "grok-4.5"]),
        ("cloudflare-ai-gateway", vec!["openai/gpt-5.6-luna"]),
    ] {
        for id in ids {
            result[provider]["models"][id] = json!({"tool_call":true,"reasoning":true,"cost":{"input":1,"output":2,"cache_read":0.1,"cache_write":0.3},"reasoning_options":[{"type":"effort","values":["none","low","high"]}]});
        }
    }
    result
}

#[test]
fn source_full_merge_supplement_clone_metadata_dedup_and_sort_workflow_matches() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for dev in [json!({}), dev_fixture()] {
        let router = json!({"data":[{"id":"moonshotai/kimi-k2.5","name":"Kimi","supported_parameters":["tools"]},{"id":"moonshotai/kimi-k2.6-extra","name":"Kimi","supported_parameters":["tools"]},{"id":"moonshotai/kimi-k3","name":"K3","supported_parameters":["tools"]},{"id":"z-ai/glm-5","name":"GLM","supported_parameters":["tools"]},{"id":"duplicate","name":"first","supported_parameters":["tools"]},{"id":"duplicate","name":"second","supported_parameters":["tools"]},{"id":"__proto__","name":"Prototype","supported_parameters":["tools"]},{"id":"constructor","name":"Constructor","supported_parameters":["tools"]}]});
        let gateway = json!({"data":[{"id":"moonshotai/kimi-k3","tags":["tool-use"],"name":"Gateway K3"}]});
        let fixtures = [
            ("https://models.dev/api.json", dev.to_string()),
            ("https://openrouter.ai/api/v1/models", router.to_string()),
            ("https://ai-gateway.vercel.sh/v1/models", gateway.to_string()),
        ];
        let http: serde_json::Map<String, Value> = fixtures
            .iter()
            .map(|(url, body)| (url.to_string(), json!({"body":body})))
            .collect();
        let expected = source.request(
            &json!({"operation":"generateModels","argv":["--json-only","--json-output","@OUTPUT@"],"http":http}),
        );
        let expected_text = expected["files"]["catalog/models.json"]
            .as_str()
            .unwrap_or_else(|| panic!("source workflow failed: {expected}"));
        let mut requests = vec![];
        let mut fetch = |url: &str| {
            requests.push(url.to_owned());
            Ok(HttpResponse {
                status: 200,
                body: fixtures.iter().find(|(key, _)| *key == url).unwrap().1.clone(),
            })
        };
        let mut generator = Generator::new(GeneratorOptions::default(), &mut fetch);
        let actual = generator.generate_catalog().unwrap();
        let expected_models = parse_lossless(expected_text).unwrap();
        assert_eq!(stringify_lossless(&actual), stringify_lossless(&expected_models));
        assert_eq!(
            requests,
            fixtures.iter().map(|(url, _)| url.to_string()).collect::<Vec<_>>()
        );
    }
}
