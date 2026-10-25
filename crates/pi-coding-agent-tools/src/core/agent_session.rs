//! Model-cycle values shared with the session runtime.

use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::Model;
use serde::Serialize;

/// `ModelCycleResult`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCycleResult {
    pub model: Model,
    #[serde(serialize_with = "serialize_thinking_level")]
    pub thinking_level: ThinkingLevel,
    pub is_scoped: bool,
}

/// Direction for `AgentSession::cycle_model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleDirection {
    Forward,
    Backward,
}

pub fn serialize_thinking_level<S: serde::Serializer>(level: &ThinkingLevel, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(level.as_str())
}
