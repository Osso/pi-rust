//! Frozen agent-core/harness/compaction/branch-summarization.ts.

use super::utils::{
    FileOperations, compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations,
    serialize_conversation,
};
use super::{complete_summary, estimate_tokens, get_message_from_entry, merge_file_details, response_text};
use crate::agent::AgentMessage;
use crate::harness::messages::convert_to_llm;
use crate::harness::session::{
    KnownSessionEntry, Session, SessionError, SessionErrorCode, SessionMetadataValue, SessionTreeEntry,
};
use crate::harness::types::{BranchSummaryError, BranchSummaryErrorCode};
use pi_ai::models::Models;
use pi_ai::types::{Model, StopReason};
use pi_js_compat::js_json::JsString;
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

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

#[derive(Debug, Clone, PartialEq)]
pub struct BranchSummaryResult {
    pub summary: JsString,
    pub read_files: Vec<JsString>,
    pub modified_files: Vec<JsString>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchSummaryDetails {
    pub read_files: Vec<JsString>,
    pub modified_files: Vec<JsString>,
}
#[derive(Debug, Clone)]
pub struct BranchPreparation {
    pub messages: Vec<AgentMessage>,
    pub file_ops: FileOperations,
    pub total_tokens: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CollectEntriesResult {
    pub entries: Vec<SessionTreeEntry>,
    pub common_ancestor_id: Option<JsString>,
}
#[derive(Clone, Copy)]
pub struct GenerateBranchSummaryOptions<'a> {
    pub models: &'a dyn Models,
    pub model: &'a Model,
    pub signal: Option<&'a CancellationToken>,
    pub custom_instructions: Option<&'a JsString>,
    pub replace_instructions: bool,
    pub reserve_tokens: Option<f64>,
}

pub async fn collect_entries_for_branch_summary<M: SessionMetadataValue>(
    session: &Session<M>,
    old_leaf_id: Option<&JsString>,
    target_id: &JsString,
) -> Result<CollectEntriesResult, SessionError> {
    let Some(old_leaf_id) = old_leaf_id.filter(|id| !id.is_empty()) else {
        return Ok(CollectEntriesResult {
            entries: Vec::new(),
            common_ancestor_id: None,
        });
    };
    let old_path = session
        .get_branch(Some(old_leaf_id))
        .await?
        .into_iter()
        .map(|entry| entry.id().clone())
        .collect::<HashSet<_>>();
    let target_path = session.get_branch(Some(target_id)).await?;
    let common_ancestor_id = target_path
        .iter()
        .rev()
        .find(|entry| old_path.contains(entry.id()))
        .map(|entry| entry.id().clone());
    let mut entries = Vec::new();
    let mut current = Some(old_leaf_id.clone());
    while let Some(id) = current.filter(|id| !id.is_empty()) {
        if Some(&id) == common_ancestor_id.as_ref() {
            break;
        }
        let entry = session
            .get_entry(&id)
            .await?
            .ok_or_else(|| SessionError::missing(SessionErrorCode::InvalidSession, &id))?;
        current = entry.parent_id().cloned();
        entries.push(entry);
    }
    entries.reverse();
    Ok(CollectEntriesResult {
        entries,
        common_ancestor_id,
    })
}

pub fn prepare_branch_entries(entries: &[SessionTreeEntry], token_budget: Option<f64>) -> BranchPreparation {
    let token_budget = token_budget.unwrap_or(0.0);
    let mut file_ops = create_file_ops();
    for entry in entries {
        if let Some(KnownSessionEntry::BranchSummary {
            from_hook,
            details: Some(details),
            ..
        }) = entry.known()
            && *from_hook != Some(true)
        {
            merge_file_details(&mut file_ops, details);
        }
    }
    let mut messages = Vec::new();
    let mut total_tokens = 0.0;
    for entry in entries.iter().rev() {
        if entry.message().is_some_and(|message| message.has_role("toolResult")) {
            continue;
        }
        let Some(message) = get_message_from_entry(entry) else {
            continue;
        };
        // Frozen extracts file operations even from the first message excluded by the budget.
        extract_file_ops_from_message(&message, &mut file_ops);
        let tokens = estimate_tokens(&message);
        if token_budget > 0.0 && total_tokens + tokens > token_budget {
            if (entry.kind() == "compaction" || entry.kind() == "branch_summary") && total_tokens < token_budget * 0.9 {
                messages.push(message);
                total_tokens += tokens;
            }
            break;
        }
        messages.push(message);
        total_tokens += tokens;
    }
    messages.reverse();
    BranchPreparation {
        messages,
        file_ops,
        total_tokens,
    }
}

pub async fn generate_branch_summary(
    entries: &[SessionTreeEntry],
    options: &GenerateBranchSummaryOptions<'_>,
) -> Result<BranchSummaryResult, BranchSummaryError> {
    let context_window = if options.model.context_window == 0.0 || options.model.context_window.is_nan() {
        128000.0
    } else {
        options.model.context_window
    };
    let preparation = prepare_branch_entries(
        entries,
        Some(context_window - options.reserve_tokens.unwrap_or(16384.0)),
    );
    if preparation.messages.is_empty() {
        return Ok(BranchSummaryResult {
            summary: "No content to summarize".into(),
            read_files: Vec::new(),
            modified_files: Vec::new(),
        });
    }
    let conversation = serialize_conversation(&convert_to_llm(&preparation.messages).map_err(|error| {
        BranchSummaryError::new(
            BranchSummaryErrorCode::SummarizationFailed,
            error.message().clone(),
            Some(Box::new(error)),
        )
    })?);
    let instructions = match options.custom_instructions.filter(|text| !text.is_empty()) {
        Some(text) if options.replace_instructions => text.clone(),
        Some(text) => JsString::from(BRANCH_SUMMARY_PROMPT) + "\n\nAdditional focus: " + text,
        None => BRANCH_SUMMARY_PROMPT.into(),
    };
    let prompt = JsString::from("<conversation>\n") + conversation + "\n</conversation>\n\n" + instructions;
    let response = complete_summary(options.models, options.model, prompt, 2048.0, options.signal)
        .await
        .map_err(|error| {
            BranchSummaryError::new(
                BranchSummaryErrorCode::SummarizationFailed,
                error.message.clone(),
                Some(Box::new(error)),
            )
        })?;
    if response.stop_reason == StopReason::Aborted {
        return Err(BranchSummaryError::new(
            BranchSummaryErrorCode::Aborted,
            response
                .error_message
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "Branch summary aborted".into()),
            None,
        ));
    }
    if response.stop_reason == StopReason::Error {
        return Err(BranchSummaryError::new(
            BranchSummaryErrorCode::SummarizationFailed,
            JsString::from("Branch summary failed: ")
                + response
                    .error_message
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| "Unknown error".into()),
            None,
        ));
    }
    let lists = compute_file_lists(&preparation.file_ops);
    let summary = JsString::from(BRANCH_SUMMARY_PREAMBLE)
        + response_text(&response)
        + format_file_operations(&lists.read_files, &lists.modified_files);
    Ok(BranchSummaryResult {
        summary,
        read_files: lists.read_files,
        modified_files: lists.modified_files,
    })
}
