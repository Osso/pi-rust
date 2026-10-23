//! Frozen `core/idle-compaction.ts` prompt-cache deadline calculation.
use crate::core::compaction::calculate_context_tokens;
use pi_ai::AssistantMessage;

// Generated from Object.getOwnPropertyNames(Object.prototype) in the pinned Node oracle.
// DISCREPANCY: the frozen plain-object TTL table exposes these inherited values;
// numeric multiplication returns NaN, not undefined.
const INHERITED_TTL_KEYS: &[&str] = &[
    "constructor",
    "__defineGetter__",
    "__defineSetter__",
    "hasOwnProperty",
    "__lookupGetter__",
    "__lookupSetter__",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toString",
    "valueOf",
    "__proto__",
    "toLocaleString",
];

pub fn compute_idle_compaction_due_at(message: &AssistantMessage) -> Option<f64> {
    let ttl = match message.api.as_str() {
        "claude-bridge" => 60.0 * 60_000.0,
        "openai-codex-responses" => 30.0 * 60_000.0,
        api if INHERITED_TTL_KEYS.contains(&api) => f64::NAN,
        _ => return None,
    };
    if calculate_context_tokens(&message.usage) < 100_000.0 {
        return None;
    }
    Some(message.timestamp + ttl * 0.9)
}
