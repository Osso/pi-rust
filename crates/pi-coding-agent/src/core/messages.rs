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

/// `isCutPointMessage`: user and assistant messages are conversational boundaries.
pub fn is_cut_point_message(message: &AgentMessage) -> bool {
    matches!(message.role(), "user" | "assistant")
}
