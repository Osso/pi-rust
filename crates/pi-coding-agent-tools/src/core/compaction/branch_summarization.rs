//! Port of `packages/coding-agent/src/core/compaction/branch-summarization.ts`.

use pi_agent_core::agent::{AgentMessage, StreamFn};
use pi_agent_core::harness::compaction::utils::{
    FileOperations, compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations,
};
use pi_agent_core::harness::messages::{
    CompactionSummaryOptions, convert_to_llm, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message,
};
use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, Model, ProviderEnv, ProviderHeaders, StopReason, TextOrImageContent, UserContent, UserMessage,
    UserRole,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue};
use serde::{Deserialize, Serialize};

use super::utils::{SUMMARIZATION_SYSTEM_PROMPT, serialize_conversation};
use super::{complete_summarization, estimate_tokens};
use crate::core::session_manager::{KnownSessionEntry, SessionEntry, SessionManager};

/// Result of a branch-summary generation attempt.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummaryResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_files: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_files: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aborted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsString>,
}

/// Details stored in `BranchSummaryEntry.details` for file tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummaryDetails {
    pub read_files: Vec<JsString>,
    pub modified_files: Vec<JsString>,
}

/// Entries prepared for branch summarization.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchPreparation {
    /// Messages extracted for summarization, in chronological order.
    pub messages: Vec<AgentMessage>,
    /// File operations extracted from tool calls.
    pub file_ops: FileOperations,
    /// Total estimated tokens in messages.
    pub total_tokens: f64,
}

/// Entries to summarize and the common ancestor with the new position.
#[derive(Debug, Clone, PartialEq)]
pub struct CollectEntriesResult {
    /// Entries to summarize, in chronological order.
    pub entries: Vec<SessionEntry>,
    /// Common ancestor between old and new position, if any.
    pub common_ancestor_id: Option<JsString>,
}

/// Options for [`generate_branch_summary`].
pub struct GenerateBranchSummaryOptions<'a> {
    pub model: &'a Model,
    pub api_key: Option<JsString>,
    pub headers: Option<ProviderHeaders>,
    pub env: Option<ProviderEnv>,
    pub signal: Option<AbortSignal>,
    pub custom_instructions: Option<&'a JsString>,
    /// When true, `custom_instructions` replaces the default prompt instead of being appended.
    pub replace_instructions: bool,
    /// Tokens reserved for prompt + LLM response (default 16384).
    pub reserve_tokens: Option<f64>,
    pub stream_fn: Option<&'a StreamFn>,
}

/// Collect entries to summarize when navigating from one position to another.
pub fn collect_entries_for_branch_summary(
    session: &SessionManager,
    old_leaf_id: Option<&JsString>,
    target_id: &JsString,
) -> CollectEntriesResult {
    let Some(old_leaf_id) = old_leaf_id else {
        return CollectEntriesResult {
            entries: Vec::new(),
            common_ancestor_id: None,
        };
    };

    let old_path: std::collections::HashSet<JsString> = session
        .get_branch(Some(old_leaf_id))
        .into_iter()
        .filter_map(|entry| entry.id().cloned())
        .collect();
    let target_path = session.get_branch(Some(target_id));

    let common_ancestor_id = target_path
        .iter()
        .rev()
        .find(|entry| entry.id().is_some_and(|id| old_path.contains(id)))
        .and_then(|entry| entry.id().cloned());

    let mut entries = Vec::new();
    let mut current = Some(old_leaf_id.to_owned());
    while let Some(id) = current {
        if Some(&id) == common_ancestor_id.as_ref() {
            break;
        }
        let Some(entry) = session.get_entry(&id) else {
            break;
        };
        entries.push(entry.clone());
        current = entry.parent_id().cloned();
    }
    entries.reverse();

    CollectEntriesResult {
        entries,
        common_ancestor_id,
    }
}

fn get_message_from_entry(entry: &SessionEntry) -> Option<AgentMessage> {
    match entry.known().as_deref() {
        Some(KnownSessionEntry::Message(entry)) => {
            if entry.message.role() == "toolResult" {
                return None;
            }
            Some((*entry.message).clone())
        }
        Some(KnownSessionEntry::CustomMessage(entry)) => Some(create_custom_message(
            &entry.custom_type,
            entry.content.clone(),
            entry.display,
            entry.details.clone(),
            &entry.timestamp,
        )),
        Some(KnownSessionEntry::BranchSummary(entry)) => Some(create_branch_summary_message(
            &entry.summary,
            &entry.from_id,
            &entry.timestamp,
        )),
        Some(KnownSessionEntry::Compaction(entry)) => Some(create_compaction_summary_message(
            &entry.summary,
            entry.tokens_before,
            &entry.timestamp,
            &CompactionSummaryOptions {
                duration_ms: entry.duration_ms,
                ..CompactionSummaryOptions::default()
            },
        )),
        _ => None,
    }
}

/// Prepare entries for summarization with a token budget.
pub fn prepare_branch_entries(entries: &[SessionEntry], token_budget: f64) -> BranchPreparation {
    let mut messages = Vec::new();
    let mut file_ops = create_file_ops();
    let mut total_tokens = 0.0;

    // First pass: cumulative file tracking from pi-generated branch summaries.
    for entry in entries {
        if let Some(KnownSessionEntry::BranchSummary(summary)) = entry.known().as_deref()
            && !summary.from_hook.unwrap_or(false)
            && let Some(details) = summary.details.as_ref()
        {
            if let Some(read_files) = details.get("readFiles").and_then(JsValue::as_array) {
                for path in read_files.iter().filter_map(JsValue::as_string) {
                    file_ops.read.insert(path.to_owned());
                }
            }
            if let Some(modified_files) = details.get("modifiedFiles").and_then(JsValue::as_array) {
                for path in modified_files.iter().filter_map(JsValue::as_string) {
                    file_ops.edited.insert(path.to_owned());
                }
            }
        }
    }

    // Second pass: walk from newest to oldest until the token budget is exhausted.
    for index in (0..entries.len()).rev() {
        let entry = &entries[index];
        let Some(message) = get_message_from_entry(entry) else {
            continue;
        };
        extract_file_ops_from_message(&message, &mut file_ops);
        let tokens = estimate_tokens(&message);

        if token_budget > 0.0 && total_tokens + tokens > token_budget {
            if (entry.entry_type() == "compaction" || entry.entry_type() == "branch_summary")
                && total_tokens < token_budget * 0.9
            {
                messages.insert(0, message);
                total_tokens += tokens;
            }
            break;
        }

        messages.insert(0, message);
        total_tokens += tokens;
    }

    BranchPreparation {
        messages,
        file_ops,
        total_tokens,
    }
}

const BRANCH_SUMMARY_PREAMBLE: &str = "The user explored a different conversation branch before returning here.
Summary of that exploration:

";

const BRANCH_SUMMARY_PROMPT: &str =
    "Create a structured summary of this conversation branch for context when returning later.

Use this EXACT format:

## Goal
[What was the user trying to accomplish in this branch?]

## Constraints & Preferences
- [Any constraints, preferences, or requirements mentioned]
- [Or \"(none)\" if none were mentioned]

## Progress
### Done
- [x] [Completed tasks/changes]

### In Progress
- [ ] [Work that was started but not finished]

### Blocked
- [Issues preventing progress, if any]

## Key Decisions
- **[Decision]**: [Brief rationale]

## Next Steps
1. [What should happen next to continue this work]

Keep each section concise. Preserve exact file paths, function names, and error messages.";

/// Generate a summary of abandoned branch entries.
pub async fn generate_branch_summary(
    entries: &[SessionEntry],
    options: &GenerateBranchSummaryOptions<'_>,
) -> Result<BranchSummaryResult, pi_agent_core::AgentError> {
    let reserve_tokens = options.reserve_tokens.unwrap_or(16_384.0);
    let context_window = if options.model.context_window > 0.0 {
        options.model.context_window
    } else {
        128_000.0
    };
    let token_budget = context_window - reserve_tokens;

    let BranchPreparation { messages, file_ops, .. } = prepare_branch_entries(entries, token_budget);

    if messages.is_empty() {
        return Ok(BranchSummaryResult {
            summary: Some("No content to summarize".into()),
            ..BranchSummaryResult::default()
        });
    }

    let converted = convert_to_llm(&messages)?;
    let conversation_text = serialize_conversation(&converted);

    let custom_instructions = options
        .custom_instructions
        .filter(|instructions| !instructions.is_empty());
    let instructions = match (options.replace_instructions, custom_instructions) {
        (true, Some(custom)) => custom.to_owned(),
        (false, Some(custom)) => JsString::from(BRANCH_SUMMARY_PROMPT) + "\n\nAdditional focus: " + custom,
        _ => BRANCH_SUMMARY_PROMPT.into(),
    };
    let prompt_text = JsString::from("<conversation>\n") + conversation_text + "\n</conversation>\n\n" + instructions;

    let summarization_message = pi_ai::types::Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(
            prompt_text,
        ))]),
        input_source: None,
        provider_native: None,
        timestamp: pi_js_compat::date::js_now_millis(),
    });

    let context = pi_ai::types::Context {
        system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.into()),
        messages: vec![summarization_message],
        tools: None,
    };
    let request_options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                signal: options.signal.clone(),
                api_key: options.api_key.clone(),
                headers: options.headers.clone(),
                env: options.env.clone(),
                ..ProviderRequestOptions::default()
            },
            max_tokens: Some(2048.0),
            ..StreamOptions::default()
        },
        ..SimpleStreamOptions::default()
    };
    let response = complete_summarization(options.model, context, request_options, options.stream_fn).await?;

    if response.stop_reason == StopReason::Aborted {
        return Ok(BranchSummaryResult {
            aborted: Some(true),
            ..BranchSummaryResult::default()
        });
    }
    if response.stop_reason == StopReason::Error {
        return Ok(BranchSummaryResult {
            error: Some(
                response
                    .error_message
                    .clone()
                    .filter(|message| !message.is_empty())
                    .unwrap_or_else(|| "Summarization failed".into()),
            ),
            ..BranchSummaryResult::default()
        });
    }

    let mut summary = JsString::join(
        response.content.iter().filter_map(|content| match content {
            AssistantContent::Text(content) => Some(&content.text),
            _ => None,
        }),
        "\n",
    );
    summary = JsString::from(BRANCH_SUMMARY_PREAMBLE) + summary;

    let file_lists = compute_file_lists(&file_ops);
    summary.push_str(format_file_operations(
        &file_lists.read_files,
        &file_lists.modified_files,
    ));

    Ok(BranchSummaryResult {
        summary: Some(if summary.is_empty() {
            "No summary generated".into()
        } else {
            summary
        }),
        read_files: Some(file_lists.read_files),
        modified_files: Some(file_lists.modified_files),
        ..BranchSummaryResult::default()
    })
}
