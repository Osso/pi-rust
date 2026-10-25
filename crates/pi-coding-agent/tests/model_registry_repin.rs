use crate::shared_oracle as oracle;

use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_js_compat::{js_json, js_json::JsValue};
use serde_json::Value;

fn empty_auth() -> AuthStorage {
    AuthStorage::in_memory(Vec::<(String, Value)>::new())
}

fn configuration_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("empty", r#"{"providers":{}}"#),
        ("null-root", "null"),
        ("missing-providers", "{}"),
        ("null-providers", r#"{"providers":null}"#),
        ("provider-array", r#"{"providers":{"local":[]}}"#),
        ("no-override", r#"{"providers":{"local":{}}}"#),
        ("key-only", r#"{"providers":{"local":{"apiKey":"private-test-key"}}}"#),
        (
            "custom-defaults",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"local-one"}]}}}"#,
        ),
        (
            "custom-endpoint-required",
            r#"{"providers":{"local":{"api":"openai-completions","models":[{"id":"local-one","baseUrl":"https://fixture.invalid"}]}}}"#,
        ),
        (
            "custom-api-required",
            r#"{"providers":{"local":{"baseUrl":"https://fixture.invalid","models":[{"id":"local-one"}]}}}"#,
        ),
        (
            "duplicate-custom",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"same","name":"first"},{"id":"same","name":"last"},{"id":"other","input":[]}]}}}"#,
        ),
        (
            "builtin-inheritance",
            r#"{"providers":{"anthropic":{"models":[{"id":"custom-inherited"}]}}}"#,
        ),
        (
            "provider-overrides",
            r#"{"providers":{"anthropic":{"baseUrl":"https://fixture.invalid/v2","headers":{"X-Provider":"provider-value"},"compat":{"supportsLongCacheRetention":false,"unknownFlag":{"nested":"retained"}}}}}"#,
        ),
        (
            "model-overrides",
            r#"{"providers":{"anthropic":{"modelOverrides":{"claude-opus-4-8":{"name":"human\ud800name","reasoning":false,"input":["image"],"cost":{"input":0},"headers":{"X-Model":"model-value"},"thinkingLevelMap":{"high":null},"compat":{"supportsLongCacheRetention":false,"unknown":"kept"}}}}}}"#,
        ),
        (
            "custom-replaces-builtin",
            r#"{"providers":{"anthropic":{"modelOverrides":{"claude-opus-4-8":{"name":"overridden","headers":{"Old":"ignored"}}},"models":[{"id":"claude-opus-4-8","name":"replacement","contextWindow":32,"maxTokens":4}]}}}"#,
        ),
        (
            "nested-compat-merge",
            r#"{"providers":{"openai":{"compat":{"openRouterRouting":{"only":["provider-one"],"max_price":{"prompt":"1.25"}},"chatTemplateKwargs":{"shared":"provider","providerOnly":true}},"modelOverrides":{"gpt-5.5":{"compat":{"openRouterRouting":{"order":["provider-two"],"max_price":{"completion":2}},"chatTemplateKwargs":{"shared":"model","modelOnly":null}}}}}}"#,
        ),
        (
            "thinking-map-unknown-custom",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"unknown-thinking","thinkingLevelMap":{"unknownBefore":{"human":"\ud800"},"high":"high","unknownAfter":[1]}}]}}}"#,
        ),
        (
            "thinking-map-unknown-override",
            r#"{"providers":{"anthropic":{"modelOverrides":{"claude-opus-4-8":{"thinkingLevelMap":{"sourceOnly":"custom","high":null}}}}}}"#,
        ),
        (
            "thinking-map-known-unit",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"known-thinking","thinkingLevelMap":{"high":"\ud800"}}]}}}"#,
        ),
        (
            "thinking-map-known-rejects-number",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"invalid-thinking","thinkingLevelMap":{"high":0}}]}}}"#,
        ),
        (
            "compat-optional-union",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"optional-union","compat":{"supportsStore":null}}]}}}"#,
        ),
        (
            "compat-common-rejects-null",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"invalid-union","compat":{"supportsLongCacheRetention":null}}]}}}"#,
        ),
        (
            "compat-human-units",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"human-compat","compat":{"unknownBefore":"human\ud800value","chatTemplateKwargs":{"arg":"native\udc00text"},"supportsStore":null,"unknownAfter":{"b":1,"a":2}}}]}}}"#,
        ),
        (
            "compat-nested-falsey",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"falsey","compat":{"openRouterRouting":0,"vercelGatewayRouting":false,"chatTemplateKwargs":""}}]}}}"#,
        ),
        (
            "compat-nested-base-spread",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","compat":{"openRouterRouting":"\ud800A","vercelGatewayRouting":["fixture"],"chatTemplateKwargs":"\ud83d\ude00"},"models":[{"id":"spread","compat":{}}]}}}"#,
        ),
        (
            "compat-legacy-opaque",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"completions","compat":{"supportsFinishReason":false,"chatTemplateArgs":{"human":"\ud800"},"supportsThinkingTokenBudget":true,"supportsOpenAIGrammarTools":true,"deferredToolsMode":"kimi","sessionAffinityFormat":"openrouter"}},{"id":"responses","api":"openai-responses","compat":{"supportsReasoningEffort":false,"sessionAffinityFormat":"openrouter","supportsStrictMode":true,"supportsOpenAIGrammarTools":true,"supportsAdditionalTools":true,"supportsToolSearch":true,"supportsExplicitPromptCacheMode":true}},{"id":"anthropic","api":"anthropic-messages","compat":{"supportsStrictTools":true,"supportsToolReferences":true}},{"id":"bedrock","api":"bedrock-converse-stream","compat":{"supportsStrictMode":true}}]}}}"#,
        ),
        (
            "custom-api-compat",
            r#"{"providers":{"local":{"api":"fixture-custom-api","baseUrl":"https://fixture.invalid","models":[{"id":"custom-api","compat":{"before":1,"supportsStore":null,"after":"kept"}}]}}}"#,
        ),
        (
            "invalid-context",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"bad","contextWindow":0}]}}}"#,
        ),
        (
            "invalid-max-tokens",
            r#"{"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"bad","maxTokens":-1}]}}}"#,
        ),
        ("invalid-id-schema", r#"{"providers":{"local":{"models":[{"id":""}]}}}"#),
        (
            "invalid-fields-schema",
            r#"{"providers":{"local":{"baseUrl":17,"models":[{"id":"bad","name":"","cost":{"input":1}}]}}}"#,
        ),
        (
            "unknown-properties",
            r#"{"ignoredRoot":true,"providers":{"local":{"api":"openai-completions","baseUrl":"https://fixture.invalid","unknownProvider":{"any":"value"},"models":[{"id":"unknown-model","unknownModel":[1,2,3]}]}}}"#,
        ),
        (
            "comments",
            "{\n // provider comment\n \"providers\": { /* no config */ }\n}",
        ),
        ("trailing-comma", r#"{"providers":{},}"#),
        ("broken-json", "{"),
    ]
}

#[tokio::test]
async fn frozen_configuration_schema_defaults_merge_and_error_snapshots_match() {
    // Frozen model-registry.ts:369-393 and403-636. Both sides read the same real
    // models.json; no schema/model field/order normalization is applied.
    let mut oracle = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let directory = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for (name, raw) in configuration_cases() {
        let path = directory.path().join(format!("{name}.json"));
        std::fs::write(&path, raw).unwrap();
        let expected: JsValue = oracle.request(&js_json!({
            "id":name,"operation":"config_snapshot","modelsPath":path.to_str().unwrap()
        }));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{name}: {expected:?}");
        let registry = ModelRegistry::create(empty_auth(), Some(path.to_str().unwrap().into())).unwrap();
        let models = registry.get_all();
        let serialized_models = serde_json::to_string(&models).unwrap();
        let actual = js_json!({"error":registry.get_error(),"models":models,"serializedModels":serialized_models});
        if actual != expected["output"] {
            eprintln!(
                "configuration mismatch {name}; source-error={:?}; native-error={:?}",
                expected["output"]["error"], actual["error"]
            );
            failures.push(js_json!({"case":name,"expected":expected["output"],"actual":actual}));
        }
    }
    let log = std::env::temp_dir().join(format!("q15e-model-registry-config-{}.json", std::process::id()));
    std::fs::write(&log, js_json::stringify_lossless(&JsValue::Array(failures.clone()))).unwrap();
    assert!(
        failures.is_empty(),
        "{} configuration differences; {}",
        failures.len(),
        log.display()
    );
}
