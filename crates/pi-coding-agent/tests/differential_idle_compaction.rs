use crate::shared_oracle as oracle;

use pi_ai::AssistantMessage;
use pi_coding_agent::core::idle_compaction::compute_idle_compaction_due_at;
use pi_js_compat::js_json::{self, JsValue};

#[test]
fn idle_due_at_matches_frozen_token_and_api_boundaries() {
    let mut oracle = oracle::TsOracle::spawn(
        "idle-compaction-oracle.ts",
        "packages/coding-agent/src/core/idle-compaction.ts",
    );
    let mut messages = Vec::new();
    let apis = [
        "claude-bridge".to_owned(),
        "openai-codex-responses".to_owned(),
        "faux".to_owned(),
        "anthropic-messages".to_owned(),
    ];
    for api in apis {
        for tokens in [99_999.0, 100_000.0, 100_001.0] {
            for timestamp in [-0.5, 1_700_000_000_000.25] {
                let mut message = AssistantMessage::empty(&api, "provider", "model", timestamp);
                message.usage.input = 40_000.0;
                message.usage.output = 10.0;
                message.usage.cache_read = tokens - 40_010.0;
                // Deliberately unrelated total: context sum, not totalTokens, is authoritative.
                message.usage.total_tokens = 1.0;
                messages.push(message);
            }
        }
    }
    let expected = oracle.request(&pi_js_compat::js_json!({"id":1,"messages":js_json::to_value(&messages).unwrap()}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let actual = messages
        .iter()
        .map(|message| match compute_idle_compaction_due_at(message) {
            None => pi_js_compat::js_json!({"kind":"undefined"}),
            Some(value) if value.is_nan() => pi_js_compat::js_json!({"kind":"nan"}),
            Some(value) => pi_js_compat::js_json!({"kind":"number","value":value}),
        })
        .collect();
    assert_eq!(expected["output"]["results"], JsValue::Array(actual));
}
