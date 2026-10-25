use crate::shared_oracle as oracle;

use pi_ai::{AssistantMessage, StopReason};
use pi_coding_agent::core::codex_quota_fallback::{CODEX_FALLBACK_PROVIDERS, is_quota_exhaustion_error};
use pi_js_compat::js_json::{self, JsString, JsValue};

#[test]
fn quota_detection_and_provider_order_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn(
        "codex-quota-oracle.ts",
        "packages/coding-agent/src/core/codex-quota-fallback.ts",
    );
    let texts = [
        "",
        "GoUsageLimitError",
        "FREEUSAGELIMITERROR",
        "usage limit",
        "available balance",
        "insufficient_quota",
        "out of budget",
        "quota exceeded",
        "billing limit",
        "billing quota",
        "billing exhausted",
        "billing exhausted soon",
        "billing  quota",
        "unrelated refusal",
        "usage\nlimit",
    ];
    let mut messages = Vec::new();
    for stop in [
        StopReason::Stop,
        StopReason::Length,
        StopReason::ToolUse,
        StopReason::Error,
        StopReason::Aborted,
    ] {
        for text in texts {
            let mut message = AssistantMessage::empty("faux", "faux", "faux", 0.0);
            message.stop_reason = stop;
            message.error_message = Some(text.into());
            messages.push(message);
        }
    }
    let mut message = AssistantMessage::empty("faux", "faux", "faux", 0.0);
    message.stop_reason = StopReason::Error;
    messages.push(message.clone());
    let mut raw = JsString::from_utf16(vec![0xd800]);
    raw.push_str("QUOTA EXCEEDED");
    message.error_message = Some(raw);
    messages.push(message);
    let response = oracle.request(&pi_js_compat::js_json!({"id":1,"messages":js_json::to_value(&messages).unwrap()}));
    assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
    let providers = CODEX_FALLBACK_PROVIDERS
        .iter()
        .map(|(provider, alternatives)| pi_js_compat::js_json!([provider, alternatives]))
        .collect::<Vec<_>>();
    assert_eq!(response["output"]["providers"], JsValue::Array(providers));
    assert_eq!(
        response["output"]["results"],
        JsValue::Array(
            messages
                .iter()
                .map(|message| JsValue::Bool(is_quota_exhaustion_error(message)))
                .collect()
        )
    );
}
