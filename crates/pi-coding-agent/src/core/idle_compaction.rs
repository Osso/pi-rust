//! Frozen `core/idle-compaction.ts` prompt-cache deadline calculation.
use crate::core::compaction::calculate_context_tokens;
use pi_ai::AssistantMessage;

pub fn compute_idle_compaction_due_at(message: &AssistantMessage) -> Option<f64> {
    let ttl = match message.api.as_str() {
        "claude-bridge" => 60.0 * 60_000.0,
        "openai-codex-responses" => 30.0 * 60_000.0,
        // DISCREPANCY: inherited functions/objects coerce to NaN, not undefined.
        api if pi_js_compat::object_prototype::inherited_property_string(api).is_some() => f64::NAN,
        _ => return None,
    };
    if calculate_context_tokens(&message.usage) < 100_000.0 {
        return None;
    }
    Some(message.timestamp + ttl * 0.9)
}
