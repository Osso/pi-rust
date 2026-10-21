//! Port of `packages/coding-agent/src/core/messages.ts`.
//!
//! The message constants, value records, constructors and LLM conversion live
//! in `pi_agent_core::harness::messages` (the agent package's identical module);
//! this module re-exports them and adds the coding-agent cut-point predicate.

pub use pi_agent_core::harness::messages::{
    BRANCH_SUMMARY_PREFIX, BRANCH_SUMMARY_SUFFIX, BashExecutionMessage, BashExecutionRole, BranchSummaryMessage,
    BranchSummaryRole, COMPACTION_SUMMARY_PREFIX, COMPACTION_SUMMARY_SUFFIX, CompactionSummaryOptions,
    CustomMessageContent, bash_execution_to_text, convert_to_llm, create_branch_summary_message,
    create_compaction_summary_message, create_custom_message,
};

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::ProviderNativeContext;
use serde::{Deserialize, Serialize};

/// Role tag for compaction-summary messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionSummaryRole {
    CompactionSummary,
}

impl CompactionSummaryRole {
    /// Returns the exact source role literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompactionSummary => "compactionSummary",
        }
    }
}

/// Value-only record for a compaction summary message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionSummaryMessage {
    #[serde(skip, default = "compaction_summary_role")]
    pub role: CompactionSummaryRole,
    pub summary: String,
    pub tokens_before: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_after: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept_from_previous_context_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compacted_result_tokens: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_native: Option<ProviderNativeContext>,
    pub timestamp: f64,
}

fn compaction_summary_role() -> CompactionSummaryRole {
    CompactionSummaryRole::CompactionSummary
}

/// `isCutPointMessage`: user and assistant messages are conversational boundaries.
pub fn is_cut_point_message(message: &AgentMessage) -> bool {
    message.has_role("user") || message.has_role("assistant")
}
