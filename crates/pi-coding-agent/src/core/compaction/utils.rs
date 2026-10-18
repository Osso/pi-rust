//! Port of `packages/coding-agent/src/core/compaction/utils.ts`.
//!
//! File-operation tracking and conversation serialization are the same as the
//! agent package's copy; this module reuses those helpers and adds the
//! coding-agent divergence: tool-call arguments are truncated to 1000
//! characters before they reach the summarization prompt.

pub use pi_agent_core::harness::compaction::SUMMARIZATION_SYSTEM_PROMPT;
pub use pi_agent_core::harness::compaction::utils::{
    FileLists, FileOperations, compute_file_lists, create_file_ops, extract_file_ops_from_message,
    format_file_operations,
};

use pi_ai::types::Message;
use serde_json::Value;

/// Maximum characters for a tool call argument value in serialized summaries.
pub const TOOL_CALL_ARGUMENT_MAX_CHARS: usize = 1000;

/// Maximum characters for a tool result in serialized summaries.
pub const TOOL_RESULT_MAX_CHARS: usize = 2000;

/// Serialize LLM messages to text for summarization.
pub fn serialize_conversation(messages: &[Message]) -> String {
    pi_agent_core::harness::compaction::utils::serialize_conversation_with_arguments(
        messages,
        &serialize_tool_call_argument,
    )
}

fn serialize_tool_call_argument(value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "[unserializable]".to_owned());
    pi_agent_core::harness::compaction::utils::truncate_for_summary(&serialized, TOOL_CALL_ARGUMENT_MAX_CHARS)
}
