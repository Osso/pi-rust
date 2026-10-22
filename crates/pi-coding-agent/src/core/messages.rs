//! Port of `packages/coding-agent/src/core/messages.ts`.
//!
//! The message constants, value records and constructors live in
//! `pi_agent_core::harness::messages` (the agent package's matching module);
//! this module re-exports them and adds the coding-agent LLM conversion, which
//! also omits already-processed images, and the cut-point predicate.

pub use pi_agent_core::harness::messages::{
    BRANCH_SUMMARY_PREFIX, BRANCH_SUMMARY_SUFFIX, BashExecutionMessage, BashExecutionRole, BranchSummaryMessage,
    BranchSummaryRole, COMPACTION_SUMMARY_PREFIX, COMPACTION_SUMMARY_SUFFIX, CompactionSummaryOptions,
    CustomMessageContent, bash_execution_to_text, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message,
};

use pi_agent_core::agent::{AgentError, AgentMessage};
use pi_ai::types::{Message, ProviderNativeContext, StopReason, TextContent, TextOrImageContent, UserContent};
use pi_js_compat::js_json::JsString;
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
    pub summary: JsString,
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

const PROCESSED_IMAGE_MARKER: &str = "[Previously processed image omitted]";

/// `convertToLlm`: maps agent messages onto provider messages, then replaces images
/// sent before the last successful assistant response with a text marker.
pub fn convert_to_llm(messages: &[AgentMessage]) -> Result<Vec<Message>, AgentError> {
    let converted = pi_agent_core::harness::messages::convert_to_llm(messages)?;
    Ok(omit_processed_images(converted))
}

fn omit_processed_images(messages: Vec<Message>) -> Vec<Message> {
    let last_successful_response = messages.iter().rposition(|message| {
        matches!(message, Message::Assistant(assistant)
            if !matches!(assistant.stop_reason, StopReason::Error | StopReason::Aborted))
    });
    let Some(last_successful_response) = last_successful_response else {
        return messages;
    };
    messages
        .into_iter()
        .enumerate()
        .map(|(index, message)| {
            if index < last_successful_response {
                omit_message_images(message)
            } else {
                message
            }
        })
        .collect()
}

fn omit_message_images(message: Message) -> Message {
    match message {
        Message::User(mut user) => {
            if let UserContent::Blocks(blocks) = &mut user.content {
                replace_images(blocks);
            }
            Message::User(user)
        }
        Message::ToolResult(mut result) => {
            replace_images(&mut result.content);
            Message::ToolResult(result)
        }
        assistant @ Message::Assistant(_) => assistant,
    }
}

fn replace_images(blocks: &mut [TextOrImageContent]) {
    for block in blocks {
        if matches!(block, TextOrImageContent::Image(_)) {
            *block = TextOrImageContent::Text(TextContent::new(PROCESSED_IMAGE_MARKER));
        }
    }
}
