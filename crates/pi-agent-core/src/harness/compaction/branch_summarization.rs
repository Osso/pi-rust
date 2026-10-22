//! Port of `packages/agent/src/harness/compaction/branch-summarization.ts`.

use std::collections::BTreeSet;

use pi_ai::models::Models;
use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    Context, Message, Model, StopReason, TextContent, TextOrImageContent, Usage, UserContent, UserMessage, UserRole,
};
use pi_ai::utils::retry::{RetryCallbacks, RetryPolicy};
use pi_ai::utils::text::{ContentText, content_text};
use pi_js_compat::js_json::{JsString, JsValue as Value};
use tokio_util::sync::CancellationToken;

use crate::agent::AgentMessage;
use crate::harness::messages::{
    CompactionSummaryOptions, convert_to_llm, create_branch_summary_message, create_compaction_summary_message,
};
use crate::harness::session::facade::Session;
use crate::harness::session::types::{
    BranchBounds, BranchQuery, Entry, EntryBody, EntryType, OptionalValue, SessionError, SessionErrorCode,
    SessionMetadataValue,
};
use crate::harness::types::{BranchSummaryError, BranchSummaryErrorCode};

use super::utils::{
    FileLists, FileOperations, compute_file_lists, create_file_ops, extract_file_ops_from_message,
    format_file_operations, serialize_conversation,
};
use super::{SUMMARIZATION_SYSTEM_PROMPT, complete_simple_with_retries, estimate_tokens, now_ms};

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

/// Generated branch summary data ready to be persisted as a branch-summary entry.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchSummaryResult {
    pub summary: JsString,
    pub usage: Option<Usage>,
    pub read_files: Vec<JsString>,
    pub modified_files: Vec<JsString>,
}

/// File-operation details stored on generated branch summary entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchSummaryDetails {
    /// Files read while exploring the summarized branch.
    pub read_files: Vec<JsString>,
    /// Files modified while exploring the summarized branch.
    pub modified_files: Vec<JsString>,
}

/// Prepared branch content for summarization.
#[derive(Debug, Clone)]
pub struct BranchPreparation {
    /// Messages selected for the branch summary.
    pub messages: Vec<AgentMessage>,
    /// File operations extracted from the branch.
    pub file_ops: FileOperations,
    /// Estimated token count for selected messages.
    pub total_tokens: f64,
}

/// Entries selected for branch summarization.
#[derive(Debug, Clone, PartialEq)]
pub struct CollectEntriesResult {
    /// Entries to summarize in chronological order.
    pub entries: Vec<Entry>,
    /// Deepest common ancestor between the previous leaf and target entry.
    pub common_ancestor_id: Option<String>,
}

/// Options for generating a branch summary.
#[derive(Clone, Copy)]
pub struct GenerateBranchSummaryOptions<'a> {
    /// Provider collection the summarization request goes through.
    pub models: &'a dyn Models,
    /// Model used for summarization.
    pub model: &'a Model,
    /// Abort signal for the summarization request.
    pub signal: Option<&'a CancellationToken>,
    /// Optional instructions appended to or replacing the default prompt.
    pub custom_instructions: Option<&'a JsString>,
    /// Replace the default prompt with custom instructions instead of appending them.
    pub replace_instructions: bool,
    /// Tokens reserved for prompt and model output. Defaults to 16384.
    pub reserve_tokens: Option<f64>,
    /// Optional retry policy for transient summarization errors.
    pub retry: Option<&'a RetryPolicy>,
}

fn branch_query(start: &str) -> BranchQuery {
    BranchQuery {
        entry: Default::default(),
        bounds: BranchBounds {
            start: Some(start.to_owned()),
            ..Default::default()
        },
    }
}

/// Collect entries that should be summarized before navigating to a different session tree entry.
pub async fn collect_entries_for_branch_summary<M: SessionMetadataValue>(
    session: &Session<M>,
    old_leaf_id: Option<&str>,
    target_id: &str,
) -> Result<CollectEntriesResult, SessionError> {
    let Some(old_leaf_id) = old_leaf_id else {
        return Ok(CollectEntriesResult {
            entries: Vec::new(),
            common_ancestor_id: None,
        });
    };
    let old_path: BTreeSet<String> = session
        .find_entries_on_branch(branch_query(old_leaf_id))
        .await?
        .into_iter()
        .map(|entry| entry.id().to_owned())
        .collect();
    let target_path = session.find_entries_on_branch(branch_query(target_id)).await?;
    let common_ancestor_id = target_path
        .iter()
        .find(|entry| old_path.contains(entry.id()))
        .map(|entry| entry.id().to_owned());

    let mut entries = Vec::new();
    let mut current = Some(old_leaf_id.to_owned());
    while let Some(id) = current {
        if Some(id.as_str()) == common_ancestor_id.as_deref() {
            break;
        }
        let entry = session
            .get_entry(&id)
            .await?
            .ok_or_else(|| SessionError::new(SessionErrorCode::InvalidEntry, format!("Entry {id} not found")))?;
        current = entry.parent_id.clone();
        entries.push(entry);
    }
    entries.reverse();

    Ok(CollectEntriesResult {
        entries,
        common_ancestor_id,
    })
}

fn get_message_from_entry(entry: &Entry) -> Option<AgentMessage> {
    match &entry.body {
        EntryBody::Message { message, .. } => (!message.has_role("toolResult")).then(|| message.clone()),
        EntryBody::BranchSummary { summary, from_id, .. } => Some(create_branch_summary_message(
            summary,
            &JsString::from(from_id.as_str()),
            entry.timestamp,
        )),
        EntryBody::Compaction {
            summary,
            tokens_before,
            duration_ms,
            ..
        } => Some(create_compaction_summary_message(
            summary,
            *tokens_before,
            entry.timestamp,
            &CompactionSummaryOptions {
                duration_ms: *duration_ms,
                ..Default::default()
            },
        )),
        _ => None,
    }
}

fn accumulate_details(details: Option<&Value>, file_ops: &mut FileOperations) {
    let Some(details) = details.and_then(Value::as_object) else {
        return;
    };
    if let Some(read_files) = details.get("readFiles").and_then(Value::as_array) {
        for file in read_files.iter().filter_map(Value::as_string) {
            file_ops.read.insert(file.clone());
        }
    }
    if let Some(modified_files) = details.get("modifiedFiles").and_then(Value::as_array) {
        for file in modified_files.iter().filter_map(Value::as_string) {
            file_ops.edited.insert(file.clone());
        }
    }
}

/// Prepare branch entries for summarization within an optional token budget.
pub fn prepare_branch_entries(entries: &[Entry], token_budget: f64) -> BranchPreparation {
    let mut file_ops = create_file_ops();
    for entry in entries {
        if let EntryBody::BranchSummary {
            details: OptionalValue::Present(details),
            ..
        } = &entry.body
        {
            accumulate_details(Some(details), &mut file_ops);
        }
    }

    let mut messages = Vec::new();
    let mut total_tokens = 0.0;
    for index in (0..entries.len()).rev() {
        let entry = &entries[index];
        let Some(message) = get_message_from_entry(entry) else {
            continue;
        };
        extract_file_ops_from_message(&message, &mut file_ops);

        let tokens = estimate_tokens(&message);
        if token_budget > 0.0 && total_tokens + tokens > token_budget {
            if matches!(entry.entry_type(), EntryType::Compaction | EntryType::BranchSummary)
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

fn branch_instructions(options: &GenerateBranchSummaryOptions<'_>) -> JsString {
    match options.custom_instructions {
        Some(instructions) if options.replace_instructions => instructions.clone(),
        Some(instructions) => JsString::from(BRANCH_SUMMARY_PROMPT) + "\n\nAdditional focus: " + instructions,
        None => BRANCH_SUMMARY_PROMPT.into(),
    }
}

/// Generate a summary for abandoned branch entries.
pub async fn generate_branch_summary(
    entries: &[Entry],
    options: &GenerateBranchSummaryOptions<'_>,
) -> Result<BranchSummaryResult, BranchSummaryError> {
    generate_branch_summary_with_callbacks(entries, options, None::<&mut super::NoRetryCallbacks>).await
}

/// [`generate_branch_summary`] with retry callbacks.
pub async fn generate_branch_summary_with_callbacks<C: RetryCallbacks + ?Sized>(
    entries: &[Entry],
    options: &GenerateBranchSummaryOptions<'_>,
    callbacks: Option<&mut C>,
) -> Result<BranchSummaryResult, BranchSummaryError> {
    let BranchPreparation { messages, file_ops, .. } = prepare_branch_entries(entries, branch_token_budget(options));

    if messages.is_empty() {
        return Ok(no_content_branch_summary());
    }

    let response = complete_simple_with_retries(
        options.models,
        options.model,
        branch_summary_context(&messages, options),
        branch_summary_completion_options(options),
        options.retry,
        callbacks,
    )
    .await;
    let (text, usage) = summarize_branch_response(&response)?;
    Ok(finish_branch_summary(text.into(), usage, &file_ops))
}

fn branch_token_budget(options: &GenerateBranchSummaryOptions<'_>) -> f64 {
    let context_window = if options.model.context_window == 0.0 {
        128000.0
    } else {
        options.model.context_window
    };
    context_window - options.reserve_tokens.unwrap_or(16384.0)
}

fn no_content_branch_summary() -> BranchSummaryResult {
    BranchSummaryResult {
        summary: "No content to summarize".into(),
        usage: None,
        read_files: Vec::new(),
        modified_files: Vec::new(),
    }
}

fn branch_summary_context(messages: &[AgentMessage], options: &GenerateBranchSummaryOptions<'_>) -> Context {
    let conversation_text = serialize_conversation(&convert_to_llm(messages));
    let prompt_text =
        JsString::from("<conversation>\n") + conversation_text + "\n</conversation>\n\n" + branch_instructions(options);
    let summarization_message = Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(prompt_text))]),
        input_source: None,
        provider_native: None,
        timestamp: now_ms(),
    });
    Context {
        system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.into()),
        messages: vec![summarization_message],
        tools: None,
    }
}

fn branch_summary_completion_options(options: &GenerateBranchSummaryOptions<'_>) -> SimpleStreamOptions {
    SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                signal: options.signal.cloned(),
                ..Default::default()
            },
            max_tokens: Some(2048.0),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn summarize_branch_response(
    response: &pi_ai::types::AssistantMessage,
) -> Result<(JsString, Usage), BranchSummaryError> {
    if response.stop_reason == StopReason::Aborted {
        return Err(BranchSummaryError::new(
            BranchSummaryErrorCode::Aborted,
            response
                .error_message
                .clone()
                .unwrap_or_else(|| "Branch summary aborted".into()),
            None,
        ));
    }
    if response.stop_reason == StopReason::Error {
        let detail = response.error_message.clone().unwrap_or_else(|| "Unknown error".into());
        return Err(BranchSummaryError::new(
            BranchSummaryErrorCode::SummarizationFailed,
            JsString::from("Branch summary failed: ") + detail,
            None,
        ));
    }
    Ok((
        content_text(ContentText::Assistant(&response.content), ""),
        response.usage.clone(),
    ))
}

fn finish_branch_summary(text: JsString, usage: Usage, file_ops: &FileOperations) -> BranchSummaryResult {
    let summary = JsString::from(BRANCH_SUMMARY_PREAMBLE) + text;
    let FileLists {
        read_files,
        modified_files,
    } = compute_file_lists(file_ops);
    let summary = summary + format_file_operations(&read_files, &modified_files);

    BranchSummaryResult {
        summary: if summary.is_empty() {
            "No summary generated".to_owned().into()
        } else {
            summary
        },
        usage: Some(usage),
        read_files,
        modified_files,
    }
}
