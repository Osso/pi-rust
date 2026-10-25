use crate::shared_oracle as oracle;

use pi_ai::api::cloudflare::{
    CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL, CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL,
    CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL, CLOUDFLARE_WORKERS_AI_BASE_URL,
};
use pi_ai::api::openai_prompt_cache::{OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH, clamp_openai_prompt_cache_key};
use serde_json::{Value, json};

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("api-leaf-oracle.ts", "packages/ai/src");
    compare_cloudflare_constants(&mut oracle);
    compare_prompt_cache_keys(&mut oracle);
}

fn compare_cloudflare_constants(oracle: &mut oracle::TsOracle) {
    let response = oracle.request(&json!({"id": 1, "operation": "cloudflare_constants"}));
    assert_eq!(response["ok"], json!(true), "cloudflare oracle failed: {response}");

    assert_eq!(response["output"]["workersAi"], json!(CLOUDFLARE_WORKERS_AI_BASE_URL));
    assert_eq!(
        response["output"]["gatewayCompat"],
        json!(CLOUDFLARE_AI_GATEWAY_COMPAT_BASE_URL)
    );
    assert_eq!(
        response["output"]["gatewayOpenai"],
        json!(CLOUDFLARE_AI_GATEWAY_OPENAI_BASE_URL)
    );
    assert_eq!(
        response["output"]["gatewayAnthropic"],
        json!(CLOUDFLARE_AI_GATEWAY_ANTHROPIC_BASE_URL)
    );
}

fn compare_prompt_cache_keys(oracle: &mut oracle::TsOracle) {
    let cases = prompt_cache_cases();
    let response = oracle.request(&json!({
        "id": 2,
        "operation": "clamp_prompt_cache_key",
        "cases": cases.iter().enumerate().map(|(index, key)| json!({"name": index.to_string(), "key": key})).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], json!(true), "prompt cache oracle failed: {response}");
    assert_eq!(
        response["output"]["maxLength"],
        json!(OPENAI_PROMPT_CACHE_KEY_MAX_LENGTH)
    );

    let results = response["output"]["results"]
        .as_array()
        .expect("clamp results must be an array");
    assert_eq!(results.len(), cases.len());
    for (result, key) in results.iter().zip(cases.iter()) {
        let actual = clamp_openai_prompt_cache_key(key.as_ref().map(pi_js_compat::js_json::JsString::from).as_ref())
            .map_or(Value::Null, |key| json!(key));
        assert_eq!(result["key"], actual, "clamp mismatch for {key:?}");
    }
}

fn prompt_cache_cases() -> Vec<Option<String>> {
    let mut cases: Vec<Option<String>> = vec![
        None,
        Some(String::new()),
        Some("prompt-cache-key".to_owned()),
        Some("a".repeat(63)),
        Some("a".repeat(64)),
        Some("a".repeat(65)),
        Some("a".repeat(200)),
        Some("🙈".repeat(64)),
        Some("🙈".repeat(65)),
        Some("é".repeat(65)),
        Some("a🙈b".repeat(30)),
        Some("line\nbreak".to_owned()),
    ];
    for length in [0, 1, 63, 64, 65, 127, 128, 129] {
        cases.push(Some("x".repeat(length)));
    }
    cases
}
