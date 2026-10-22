//! Port of `packages/agent/src/harness/compaction/compaction.ts`.

use std::time::{SystemTime, UNIX_EPOCH};

use pi_ai::models::Models;
use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantMessage, CacheRetention, Context, Message, Model, StopReason, TextContent, TextOrImageContent, Usage,
    UsageCost, UserContent, UserMessage, UserRole,
};
use pi_ai::utils::estimate::{
    TextOrImageInput, estimate_assistant_message_tokens, estimate_text_and_image_content_tokens,
};
use pi_ai::utils::retry::{RetryCallbacks, RetryPolicy, retry_assistant_call};
use pi_ai::utils::text::{ContentText, content_text};
use pi_ai::utils::uuid::uuidv7;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::agent::AgentMessage;
use crate::agent::types::reasoning_for;
use crate::harness::messages::{
    CompactionSummaryOptions, convert_to_llm, create_branch_summary_message, create_compaction_summary_message,
};
use crate::harness::session::context::{SessionContextBuildOptions, build_session_context};
use crate::harness::session::types::{Entry, EntryBody, EntryType};
use crate::harness::types::{CompactionError, CompactionErrorCode, ThinkingLevel};

use self::utils::{compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations};

pub mod branch_summarization;
pub mod utils;

pub use branch_summarization::{
    BranchPreparation, BranchSummaryDetails, BranchSummaryResult, CollectEntriesResult, GenerateBranchSummaryOptions,
    collect_entries_for_branch_summary, generate_branch_summary, prepare_branch_entries,
};
pub use pi_ai::utils::estimate::calculate_context_tokens;
pub use utils::{FileLists, FileOperations, serialize_conversation};

const ESTIMATED_IMAGE_CHARS: usize = 4800;
const MAX_COMPACTION_CONTEXT_CHARS: usize = 300_000;
const MAX_PROTECTED_RECENT_CONTEXT_MESSAGES: usize = 3;
const MAX_PROTECTED_RECENT_CONTEXT_TOKENS: f64 = 10_000.0;

/// Exact system prompt used for conversation summarization.
pub const SUMMARIZATION_SYSTEM_PROMPT: &str = "You are a context summarization assistant. Your task is to read a conversation between a user and an AI assistant, then produce a structured summary following the exact format specified.\n\nDo NOT continue the conversation. Do NOT respond to any questions in the conversation. ONLY output the structured summary.";

const SUMMARIZATION_PROMPT: &str = "The messages above are a conversation to summarize. Create a structured context checkpoint summary that another LLM will use to continue the work.

Use this EXACT format:

## Goal
[What is the user trying to accomplish? Can be multiple items if the session covers different tasks.]

## Constraints & Preferences
- [Any constraints, preferences, or requirements mentioned by user]
- [Or \"(none)\" if none were mentioned]

## Progress
### Done
- [x] [Completed tasks/changes]

### In Progress
- [ ] [Current work]

### Blocked
- [Issues preventing progress, if any]

## Key Decisions
- **[Decision]**: [Brief rationale]

## Next Steps
1. [Ordered list of what should happen next]

## Critical Context
- [Any data, examples, or references needed to continue]
- [Or \"(none)\" if not applicable]

Keep each section concise. Preserve exact file paths, function names, and error messages.";

const UPDATE_SUMMARIZATION_PROMPT: &str = "The messages above are NEW conversation messages to incorporate into the existing summary provided in <previous-summary> tags.

Update the existing structured summary with new information. RULES:
- PRESERVE all existing information from the previous summary
- ADD new progress, decisions, and context from the new messages
- UPDATE the Progress section: move items from \"In Progress\" to \"Done\" when completed
- UPDATE \"Next Steps\" based on what was accomplished
- PRESERVE exact file paths, function names, and error messages
- If something is no longer relevant, you may remove it

Use this EXACT format:

## Goal
[Preserve existing goals, add new ones if the task expanded]

## Constraints & Preferences
- [Preserve existing, add new ones discovered]

## Progress
### Done
- [x] [Include previously done items AND newly completed items]

### In Progress
- [ ] [Current work - update based on progress]

### Blocked
- [Current blockers - remove if resolved]

## Key Decisions
- **[Decision]**: [Brief rationale] (preserve all previous, add new)

## Next Steps
1. [Update based on current state]

## Critical Context
- [Preserve important context, add new if needed]

Keep each section concise. Preserve exact file paths, function names, and error messages.";

const TURN_PREFIX_SUMMARIZATION_PROMPT: &str =
    "This is the PREFIX of a turn that was too large to keep. The SUFFIX (recent work) is retained.

Summarize the prefix to provide context for the retained suffix:

## Original Request
[What did the user ask for in this turn?]

## Early Progress
- [Key decisions and work done in the prefix]

## Context for Suffix
- [Information needed to understand the retained recent work]

Be concise. Focus on what's needed to understand the kept suffix.";

/// Cut point selected for compaction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutPointResult {
    pub first_kept_entry_index: f64,
    pub turn_start_index: f64,
    pub is_split_turn: bool,
}

/// File-operation details stored on generated compaction entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompactionDetails {
    pub read_files: Vec<JsString>,
    pub modified_files: Vec<JsString>,
}

/// Compaction thresholds and retention settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionSettings {
    pub enabled: bool,
    pub reserve_tokens: f64,
    pub keep_recent_tokens: f64,
}

/// Default compaction settings used by the harness.
pub const DEFAULT_COMPACTION_SETTINGS: CompactionSettings = CompactionSettings {
    enabled: true,
    reserve_tokens: 16384.0,
    keep_recent_tokens: 20000.0,
};

/// Estimated context-token usage for a message list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextUsageEstimate {
    pub tokens: f64,
    pub usage_tokens: f64,
    pub trailing_tokens: f64,
    pub last_usage_index: Option<f64>,
}

/// Return whether context usage exceeds the configured compaction threshold.
pub fn should_compact(context_tokens: f64, context_window: f64, settings: &CompactionSettings) -> bool {
    if !settings.enabled {
        return false;
    }

    context_tokens > context_window - settings.reserve_tokens
}

/// Estimate token count for one message using a conservative character heuristic.
pub fn estimate_tokens(message: &AgentMessage) -> f64 {
    match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => estimate_text_and_image_content_tokens(TextOrImageInput::Utf16(text)),
            UserContent::Blocks(blocks) => estimate_text_and_image_content_tokens(TextOrImageInput::Blocks(blocks)),
        },
        AgentMessage::Assistant(assistant) => estimate_assistant_message_tokens(assistant),
        AgentMessage::ToolResult(result) => {
            estimate_text_and_image_content_tokens(TextOrImageInput::Blocks(&result.content))
        }
        AgentMessage::Custom(object) => estimate_custom_tokens(object),
    }
}

fn estimate_custom_tokens(object: &JsObject) -> f64 {
    let Some(role) = object.get("role").and_then(Value::as_string) else {
        return 0.0;
    };
    if role == "custom" || role == "toolResult" {
        ceil_tokens(custom_content_chars(object.get("content")))
    } else if role == "bashExecution" {
        ceil_tokens(js_len(string_field(object, "command")) + js_len(string_field(object, "output")))
    } else if role == "branchSummary" || role == "compactionSummary" {
        ceil_tokens(js_len(string_field(object, "summary")))
    } else {
        0.0
    }
}

fn custom_content_chars(content: Option<&Value>) -> usize {
    match content {
        Some(Value::String(text)) => text.len(),
        Some(Value::Array(blocks)) => blocks.iter().map(content_block_chars).sum(),
        _ => 0,
    }
}

fn content_block_chars(block: &Value) -> usize {
    let Some(kind) = block.get("type").and_then(Value::as_string) else {
        return 0;
    };
    if kind == "text" {
        block
            .get("text")
            .and_then(Value::as_string)
            .map(JsString::len)
            .unwrap_or(0)
    } else if kind == "image" {
        ESTIMATED_IMAGE_CHARS
    } else {
        0
    }
}

fn ceil_tokens(chars: usize) -> f64 {
    (chars as f64 / 4.0).ceil()
}

fn string_field(object: &JsObject, key: &str) -> JsString {
    object.get(key).and_then(Value::as_string).cloned().unwrap_or_default()
}

fn js_len(text: impl Into<JsString>) -> usize {
    text.into().len()
}

fn get_assistant_usage(message: &AgentMessage) -> Option<&Usage> {
    let assistant = message.as_assistant()?;
    if matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error) {
        return None;
    }
    (calculate_context_tokens(&assistant.usage) > 0.0).then_some(&assistant.usage)
}

/// Return usage from the last valid assistant message in session entries.
pub fn get_last_assistant_usage(entries: &[Entry]) -> Option<&Usage> {
    entries.iter().rev().find_map(|entry| match &entry.body {
        EntryBody::Message { message, .. } => get_assistant_usage(message),
        _ => None,
    })
}

fn get_last_assistant_usage_info(messages: &[AgentMessage]) -> Option<(usize, &Usage)> {
    messages
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, message)| get_assistant_usage(message).map(|usage| (index, usage)))
}

/// Estimate context tokens for messages using provider usage when available.
pub fn estimate_context_tokens(messages: &[AgentMessage]) -> ContextUsageEstimate {
    let Some((index, usage)) = get_last_assistant_usage_info(messages) else {
        let estimated: f64 = messages.iter().map(estimate_tokens).sum();
        return ContextUsageEstimate {
            tokens: estimated,
            usage_tokens: 0.0,
            trailing_tokens: estimated,
            last_usage_index: None,
        };
    };

    let usage_tokens = calculate_context_tokens(usage);
    let trailing_tokens: f64 = messages[index + 1..].iter().map(estimate_tokens).sum();

    ContextUsageEstimate {
        tokens: usage_tokens + trailing_tokens,
        usage_tokens,
        trailing_tokens,
        last_usage_index: Some(index as f64),
    }
}

fn get_message_from_entry(entry: &Entry) -> Option<AgentMessage> {
    match &entry.body {
        EntryBody::Message { message, .. } => Some(message.clone()),
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

fn get_message_from_entry_for_compaction(entry: &Entry) -> Option<AgentMessage> {
    if entry.entry_type() == EntryType::Compaction {
        return None;
    }
    get_message_from_entry(entry)
}

fn estimate_entry_tokens(entry: &Entry) -> Option<f64> {
    get_message_from_entry(entry).map(|message| estimate_tokens(&message))
}

fn is_turn_start_entry(entry: &Entry) -> bool {
    if entry.entry_type() == EntryType::BranchSummary {
        return true;
    }
    match &entry.body {
        EntryBody::Message { message, .. } => ["user", "bashExecution", "custom"]
            .iter()
            .any(|role| message.has_role(role)),
        _ => false,
    }
}

fn is_active_turn_continuation_entry(entry: &Entry) -> bool {
    match &entry.body {
        EntryBody::Message { message, .. } => ["assistant", "toolResult"].iter().any(|role| message.has_role(role)),
        _ => false,
    }
}

fn find_context_message_count_cut_index(entries: &[Entry], start_index: usize, end_index: usize) -> usize {
    let mut message_count = 0;
    for index in (start_index..end_index).rev() {
        if estimate_entry_tokens(&entries[index]).is_none() {
            continue;
        }
        message_count += 1;
        if message_count == MAX_PROTECTED_RECENT_CONTEXT_MESSAGES {
            return index;
        }
    }
    start_index
}

fn find_token_bounded_cut_index(entries: &[Entry], start_index: usize, end_index: usize) -> usize {
    let mut accumulated_tokens = 0.0;
    let mut suffix_start_index = start_index;
    let mut has_context_message = false;

    for index in (start_index..end_index).rev() {
        let Some(tokens) = estimate_entry_tokens(&entries[index]) else {
            continue;
        };
        if has_context_message && accumulated_tokens + tokens > MAX_PROTECTED_RECENT_CONTEXT_TOKENS {
            break;
        }
        has_context_message = true;
        accumulated_tokens += tokens;
        suffix_start_index = index;
    }

    suffix_start_index
}

fn find_nearest_cut_point_at_or_after(cut_points: &[usize], index: usize) -> usize {
    cut_points
        .iter()
        .copied()
        .find(|cut_point| *cut_point >= index)
        .or_else(|| cut_points.last().copied())
        .unwrap_or(index)
}

fn find_protected_suffix_start_index(entries: &[Entry], start_index: usize, end_index: usize) -> usize {
    for index in (start_index..end_index).rev() {
        if entries[index].entry_type() == EntryType::Compaction {
            return index + 1;
        }
    }
    start_index
}

fn find_active_turn_floor_index(entries: &[Entry], start_index: usize, end_index: usize) -> Option<usize> {
    let mut has_continuation = false;
    for index in (start_index..end_index).rev() {
        let entry = &entries[index];
        if is_turn_start_entry(entry) {
            return has_continuation.then_some(index);
        }
        if is_active_turn_continuation_entry(entry) {
            has_continuation = true;
        }
    }
    None
}

fn find_protected_suffix_cut_index(
    entries: &[Entry],
    start_index: usize,
    end_index: usize,
    cut_points: &[usize],
) -> usize {
    let protected_start_index = find_protected_suffix_start_index(entries, start_index, end_index);
    let message_count_cut_index = find_context_message_count_cut_index(entries, protected_start_index, end_index);
    let token_bounded_cut_index = find_token_bounded_cut_index(entries, protected_start_index, end_index);
    let mut shorter_suffix_cut_index = message_count_cut_index.max(token_bounded_cut_index);

    if let Some(active_turn_floor_index) = find_active_turn_floor_index(entries, protected_start_index, end_index)
        && let Some(&first_cut_point_index) = cut_points.first()
        && first_cut_point_index < active_turn_floor_index
    {
        shorter_suffix_cut_index = shorter_suffix_cut_index.min(active_turn_floor_index);
    }

    find_nearest_cut_point_at_or_after(cut_points, shorter_suffix_cut_index)
}

fn find_valid_cut_points(entries: &[Entry], start_index: usize, end_index: usize) -> Vec<usize> {
    let mut cut_points = Vec::new();
    for (index, entry) in entries.iter().enumerate().take(end_index).skip(start_index) {
        if let EntryBody::Message { message, .. } = &entry.body
            && [
                "bashExecution",
                "custom",
                "branchSummary",
                "compactionSummary",
                "user",
                "assistant",
            ]
            .iter()
            .any(|role| message.has_role(role))
        {
            cut_points.push(index);
        }
        if entry.entry_type() == EntryType::BranchSummary {
            cut_points.push(index);
        }
    }
    cut_points
}

/// Find the user-visible message that starts the turn containing an entry.
pub fn find_turn_start_index(entries: &[Entry], entry_index: usize, start_index: usize) -> f64 {
    find_turn_start_index_option(entries, entry_index, start_index).map_or(-1.0, |index| index as f64)
}

fn find_turn_start_index_option(entries: &[Entry], entry_index: usize, start_index: usize) -> Option<usize> {
    if entry_index < start_index {
        return None;
    }
    (start_index..=entry_index)
        .rev()
        .find(|index| is_turn_start_entry(&entries[*index]))
}

/// Find the compaction cut point that keeps the bounded protected suffix and the latest active turn.
pub fn find_cut_point(
    entries: &[Entry],
    start_index: usize,
    end_index: usize,
    _keep_recent_tokens: f64,
) -> CutPointResult {
    let cut_points = find_valid_cut_points(entries, start_index, end_index);

    if cut_points.is_empty() {
        return CutPointResult {
            first_kept_entry_index: start_index as f64,
            turn_start_index: -1.0,
            is_split_turn: false,
        };
    }

    let mut cut_index = find_protected_suffix_cut_index(entries, start_index, end_index, &cut_points);
    while cut_index > start_index {
        let previous = &entries[cut_index - 1];
        if matches!(
            previous.entry_type(),
            EntryType::Compaction | EntryType::Message | EntryType::BranchSummary
        ) {
            break;
        }
        cut_index -= 1;
    }

    let cut_entry = &entries[cut_index];
    let is_user_message = cut_entry.entry_type() == EntryType::Message
        && cut_entry.body.message().is_some_and(|message| message.role() == "user");
    let turn_start_index = if is_user_message {
        None
    } else {
        find_turn_start_index_option(entries, cut_index, start_index)
    };

    CutPointResult {
        first_kept_entry_index: cut_index as f64,
        turn_start_index: turn_start_index.map_or(-1.0, |index| index as f64),
        is_split_turn: !is_user_message && turn_start_index.is_some(),
    }
}

/// Prepared inputs for a compaction run.
#[derive(Debug, Clone)]
pub struct CompactionPreparation {
    pub messages_to_summarize: Vec<AgentMessage>,
    pub turn_prefix_messages: Vec<AgentMessage>,
    pub retained_tail: Vec<AgentMessage>,
    pub is_split_turn: bool,
    pub tokens_before: f64,
    pub previous_summary: Option<JsString>,
    pub file_ops: FileOperations,
    pub settings: CompactionSettings,
}

fn virtual_retained_entries(compaction: &Entry) -> Vec<Entry> {
    let EntryBody::Compaction { retained_tail, .. } = &compaction.body else {
        return Vec::new();
    };
    retained_tail
        .iter()
        .enumerate()
        .map(|(index, message)| Entry {
            body: EntryBody::Message {
                id: format!("{}:retained:{}", compaction.id(), index),
                message: message.clone(),
                terminate: None,
            },
            parent_id: Some(if index == 0 {
                compaction.id().to_owned()
            } else {
                format!("{}:retained:{}", compaction.id(), index - 1)
            }),
            seq: compaction.seq,
            timestamp: message.timestamp().unwrap_or(0),
        })
        .collect()
}

fn extract_file_operations(
    messages: &[AgentMessage],
    entries: &[Entry],
    previous_compaction_index: Option<usize>,
) -> FileOperations {
    let mut file_ops = create_file_ops();
    if let Some(index) = previous_compaction_index
        && let EntryBody::Compaction {
            details: crate::harness::session::types::OptionalValue::Present(details),
            ..
        } = &entries[index].body
        && let Some(details) = details.as_object()
    {
        if let Some(read_files) = details.get("readFiles").and_then(Value::as_array) {
            for file in read_files.iter().filter_map(Value::as_string) {
                file_ops.read.insert(file.to_owned());
            }
        }
        if let Some(modified_files) = details.get("modifiedFiles").and_then(Value::as_array) {
            for file in modified_files.iter().filter_map(Value::as_string) {
                file_ops.edited.insert(file.to_owned());
            }
        }
    }
    for message in messages {
        extract_file_ops_from_message(message, &mut file_ops);
    }
    file_ops
}

/// Prepare session entries for compaction, or return `None` when compaction is not applicable.
pub fn prepare_compaction(
    path_entries: &[Entry],
    settings: &CompactionSettings,
) -> Result<Option<CompactionPreparation>, CompactionError> {
    if !is_compactable(path_entries) {
        return Ok(None);
    }

    let previous_compaction_index = previous_compaction_index(path_entries);
    let previous_summary = previous_summary(path_entries, previous_compaction_index);
    let compactable_entries = compactable_entries(path_entries, previous_compaction_index);
    let boundary_end = compactable_entries.len();

    let tokens_before =
        estimate_context_tokens(&build_session_context(path_entries, &SessionContextBuildOptions::default()).messages)
            .tokens;
    let cut_point = find_cut_point(&compactable_entries, 0, boundary_end, settings.keep_recent_tokens);
    let (messages_to_summarize, turn_prefix_messages, retained_tail) =
        split_at_cut_point(&compactable_entries, &cut_point, boundary_end);

    if messages_to_summarize.is_empty() && turn_prefix_messages.is_empty() {
        return Ok(None);
    }

    Ok(Some(CompactionPreparation {
        file_ops: collect_file_operations(
            &messages_to_summarize,
            &turn_prefix_messages,
            path_entries,
            previous_compaction_index,
            cut_point.is_split_turn,
        ),
        messages_to_summarize,
        turn_prefix_messages,
        retained_tail,
        is_split_turn: cut_point.is_split_turn,
        tokens_before,
        previous_summary,
        settings: *settings,
    }))
}

fn is_compactable(path_entries: &[Entry]) -> bool {
    path_entries
        .last()
        .is_some_and(|entry| entry.entry_type() != EntryType::Compaction)
}

fn previous_compaction_index(path_entries: &[Entry]) -> Option<usize> {
    path_entries
        .iter()
        .rposition(|entry| entry.entry_type() == EntryType::Compaction)
}

fn previous_summary(path_entries: &[Entry], previous_compaction_index: Option<usize>) -> Option<JsString> {
    let index = previous_compaction_index?;
    match &path_entries[index].body {
        EntryBody::Compaction { summary, .. } => Some(summary.clone()),
        _ => None,
    }
}

fn compactable_entries(path_entries: &[Entry], previous_compaction_index: Option<usize>) -> Vec<Entry> {
    let Some(index) = previous_compaction_index else {
        return path_entries.to_vec();
    };
    let mut entries = virtual_retained_entries(&path_entries[index]);
    entries.extend_from_slice(&path_entries[index + 1..]);
    entries
}

fn split_at_cut_point(
    entries: &[Entry],
    cut_point: &CutPointResult,
    boundary_end: usize,
) -> (Vec<AgentMessage>, Vec<AgentMessage>, Vec<AgentMessage>) {
    let first_kept_entry_index = cut_point.first_kept_entry_index as usize;
    let history_end = if cut_point.is_split_turn {
        cut_point.turn_start_index as usize
    } else {
        first_kept_entry_index
    };

    let messages_to_summarize = collect_messages(&entries[..history_end]);
    let turn_prefix_messages = if cut_point.is_split_turn {
        collect_messages(&entries[history_end..first_kept_entry_index])
    } else {
        Vec::new()
    };
    let retained_tail = collect_messages(&entries[first_kept_entry_index..boundary_end]);
    (messages_to_summarize, turn_prefix_messages, retained_tail)
}

fn collect_file_operations(
    messages_to_summarize: &[AgentMessage],
    turn_prefix_messages: &[AgentMessage],
    path_entries: &[Entry],
    previous_compaction_index: Option<usize>,
    is_split_turn: bool,
) -> FileOperations {
    let mut file_ops = extract_file_operations(messages_to_summarize, path_entries, previous_compaction_index);
    if is_split_turn {
        for message in turn_prefix_messages {
            extract_file_ops_from_message(message, &mut file_ops);
        }
    }
    file_ops
}

fn collect_messages(entries: &[Entry]) -> Vec<AgentMessage> {
    entries
        .iter()
        .filter_map(get_message_from_entry_for_compaction)
        .collect()
}

/// Options for generating or updating a conversation summary.
#[derive(Clone, Copy)]
pub struct GenerateSummaryOptions<'a> {
    pub current_messages: &'a [AgentMessage],
    pub models: &'a dyn Models,
    pub model: &'a Model,
    pub reserve_tokens: f64,
    pub signal: Option<&'a CancellationToken>,
    pub custom_instructions: Option<&'a JsString>,
    pub previous_summary: Option<&'a JsString>,
    pub thinking_level: Option<ThinkingLevel>,
    pub retry: Option<&'a RetryPolicy>,
}

/// Generated summary text with the provider usage that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryWithUsage {
    pub text: JsString,
    pub usage: Usage,
}

/// Generated compaction data ready to be persisted as a compaction entry.
#[derive(Debug, Clone)]
pub struct CompactResult {
    pub summary: JsString,
    pub tokens_before: f64,
    pub duration_ms: Option<f64>,
    pub usage: Option<Usage>,
    pub retained_tail: Vec<AgentMessage>,
    pub details: Option<CompactionDetails>,
}

/// Options for a compaction run.
#[derive(Clone, Copy, Default)]
pub struct CompactOptions<'a> {
    pub custom_instructions: Option<&'a JsString>,
    pub signal: Option<&'a CancellationToken>,
    pub thinking_level: Option<ThinkingLevel>,
    pub retry: Option<&'a RetryPolicy>,
}

/// Retry callbacks that ignore every event.
pub struct NoRetryCallbacks;

impl RetryCallbacks for NoRetryCallbacks {}

pub(super) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn build_summary_prompt(options: &GenerateSummaryOptions<'_>) -> JsString {
    let summary_prompt = if options.previous_summary.is_some() {
        UPDATE_SUMMARIZATION_PROMPT
    } else {
        SUMMARIZATION_PROMPT
    };
    let focused = match options.custom_instructions {
        Some(instructions) => JsString::from(summary_prompt) + "\n\nAdditional focus: " + instructions,
        None => summary_prompt.into(),
    };
    let conversation = truncate_utf16(
        &serialize_conversation(&convert_to_llm(options.current_messages)),
        MAX_COMPACTION_CONTEXT_CHARS,
    );
    let previous = match options.previous_summary {
        Some(summary) => JsString::from("<previous-summary>\n") + summary + "\n</previous-summary>\n\n",
        None => JsString::default(),
    };
    JsString::from("<conversation>\n") + conversation + "\n</conversation>\n\n" + previous + focused
}

fn truncate_utf16(text: &JsString, max_chars: usize) -> JsString {
    JsString::from_utf16(text.units()[..text.len().min(max_chars)].to_vec())
}

fn output_token_cap(model: &Model, fraction: f64, reserve_tokens: f64) -> f64 {
    let requested = (fraction * reserve_tokens).floor();
    let model_cap = if model.max_tokens > 0.0 {
        model.max_tokens
    } else {
        f64::INFINITY
    };
    requested.min(model_cap)
}

fn reasoning_option(model: &Model, thinking_level: Option<ThinkingLevel>) -> Option<pi_ai::types::ThinkingLevel> {
    match thinking_level {
        Some(level) if model.reasoning && level != ThinkingLevel::Off => reasoning_for(level),
        _ => None,
    }
}

fn build_summary_request(options: &GenerateSummaryOptions<'_>) -> (Context, SimpleStreamOptions) {
    let max_tokens = output_token_cap(options.model, 0.8, options.reserve_tokens);
    let message = Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(build_summary_prompt(
            options,
        )))]),
        input_source: None,
        provider_native: None,
        timestamp: now_ms(),
    });
    let completion_options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                signal: options.signal.cloned(),
                ..Default::default()
            },
            max_tokens: Some(max_tokens),
            ..Default::default()
        },
        reasoning: reasoning_option(options.model, options.thinking_level),
        ..Default::default()
    };
    (
        Context {
            system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.into()),
            messages: vec![message],
            tools: None,
        },
        completion_options,
    )
}

fn stream_failure_message(model: &Model, failure: pi_ai::utils::event_stream::StreamFailure) -> AssistantMessage {
    let mut message = AssistantMessage::empty(model.api.clone(), model.provider.clone(), model.id.clone(), now_ms());
    message.stop_reason = StopReason::Error;
    message.error_message = Some(failure.message().to_owned());
    message
}

pub async fn complete_simple_with_retries<C: RetryCallbacks + ?Sized>(
    models: &dyn Models,
    model: &Model,
    context: Context,
    options: SimpleStreamOptions,
    retry: Option<&RetryPolicy>,
    callbacks: Option<&mut C>,
) -> AssistantMessage {
    // Summaries are standalone requests, so isolate routing and avoid cache writes that cannot be reused.
    let mut request_options = options;
    request_options.stream.cache_retention = Some(CacheRetention::None);
    request_options.stream.session_id = Some(uuidv7());
    let signal = request_options.stream.request.signal.clone();

    let produce = || {
        let context = context.clone();
        let options = request_options.clone();
        let model = model.clone();
        async move {
            models
                .complete_simple(model.clone(), context, options)
                .await
                .unwrap_or_else(|failure| stream_failure_message(&model, failure))
        }
    };
    retry_assistant_call(produce, retry, signal.as_ref(), callbacks).await
}

fn summarize_response(
    response: &AssistantMessage,
    aborted_message: &str,
    failed_prefix: &str,
) -> Result<(JsString, Usage), CompactionError> {
    if response.stop_reason == StopReason::Aborted {
        return Err(CompactionError::new(
            CompactionErrorCode::Aborted,
            response.error_message.clone().unwrap_or_else(|| aborted_message.into()),
            None,
        ));
    }
    if response.stop_reason == StopReason::Error {
        let detail = response.error_message.clone().unwrap_or_else(|| "Unknown error".into());
        return Err(CompactionError::new(
            CompactionErrorCode::SummarizationFailed,
            JsString::from(failed_prefix) + detail,
            None,
        ));
    }
    Ok((
        content_text(ContentText::Assistant(&response.content), ""),
        response.usage.clone(),
    ))
}

async fn generate_summary_with_options<C: RetryCallbacks + ?Sized>(
    options: &GenerateSummaryOptions<'_>,
    callbacks: Option<&mut C>,
) -> Result<SummaryWithUsage, CompactionError> {
    let (context, completion_options) = build_summary_request(options);
    let response = complete_simple_with_retries(
        options.models,
        options.model,
        context,
        completion_options,
        options.retry,
        callbacks,
    )
    .await;
    let (text, usage) = summarize_response(&response, "Summarization aborted", "Summarization failed: ")?;
    Ok(SummaryWithUsage { text, usage })
}

/// Generate or update a conversation summary for compaction.
pub async fn generate_summary(options: &GenerateSummaryOptions<'_>) -> Result<JsString, CompactionError> {
    generate_summary_with_callbacks(options, None::<&mut NoRetryCallbacks>)
        .await
        .map(|summary| summary.text)
}

/// [`generate_summary`] with retry callbacks.
pub async fn generate_summary_with_callbacks<C: RetryCallbacks + ?Sized>(
    options: &GenerateSummaryOptions<'_>,
    callbacks: Option<&mut C>,
) -> Result<SummaryWithUsage, CompactionError> {
    generate_summary_with_options(options, callbacks).await
}

/// Generate or update a conversation summary and return its provider usage.
pub async fn generate_summary_with_usage(
    options: &GenerateSummaryOptions<'_>,
) -> Result<SummaryWithUsage, CompactionError> {
    generate_summary_with_options(options, None::<&mut NoRetryCallbacks>).await
}

/// [`generate_summary_with_usage`] with retry callbacks.
pub async fn generate_summary_with_usage_with_callbacks<C: RetryCallbacks + ?Sized>(
    options: &GenerateSummaryOptions<'_>,
    callbacks: Option<&mut C>,
) -> Result<SummaryWithUsage, CompactionError> {
    generate_summary_with_options(options, callbacks).await
}

fn combine_usage(first: &Usage, second: &Usage) -> Usage {
    Usage {
        input: first.input + second.input,
        output: first.output + second.output,
        cache_read: first.cache_read + second.cache_read,
        cache_write: first.cache_write + second.cache_write,
        cache_write_1h: combine_optional(first.cache_write_1h, second.cache_write_1h),
        reasoning: combine_optional(first.reasoning, second.reasoning),
        total_tokens: first.total_tokens + second.total_tokens,
        cost: UsageCost {
            input: first.cost.input + second.cost.input,
            output: first.cost.output + second.cost.output,
            cache_read: first.cost.cache_read + second.cost.cache_read,
            cache_write: first.cost.cache_write + second.cost.cache_write,
            total: first.cost.total + second.cost.total,
        },
    }
}

fn combine_optional(first: Option<f64>, second: Option<f64>) -> Option<f64> {
    (first.is_some() || second.is_some()).then(|| first.unwrap_or(0.0) + second.unwrap_or(0.0))
}

/// Generate compaction summary data from prepared session history.
pub async fn compact(
    preparation: &CompactionPreparation,
    models: &dyn Models,
    model: &Model,
    options: &CompactOptions<'_>,
) -> Result<CompactResult, CompactionError> {
    compact_with_callbacks(preparation, models, model, options, None::<&mut NoRetryCallbacks>).await
}

/// [`compact`] with retry callbacks.
pub async fn compact_with_callbacks<C: RetryCallbacks + ?Sized>(
    preparation: &CompactionPreparation,
    models: &dyn Models,
    model: &Model,
    options: &CompactOptions<'_>,
    callbacks: Option<&mut C>,
) -> Result<CompactResult, CompactionError> {
    let started_at = now_ms();
    let (summary, summary_usage) = summarize_preparation(preparation, models, model, options, callbacks).await?;

    let FileLists {
        read_files,
        modified_files,
    } = compute_file_lists(&preparation.file_ops);
    let summary = summary + format_file_operations(&read_files, &modified_files);

    Ok(CompactResult {
        summary,
        tokens_before: preparation.tokens_before,
        duration_ms: Some((now_ms() - started_at) as f64),
        usage: Some(summary_usage),
        retained_tail: preparation.retained_tail.clone(),
        details: Some(CompactionDetails {
            read_files,
            modified_files,
        }),
    })
}

fn preparation_summary_options<'a>(
    preparation: &'a CompactionPreparation,
    models: &'a dyn Models,
    model: &'a Model,
    options: &'a CompactOptions<'a>,
) -> GenerateSummaryOptions<'a> {
    GenerateSummaryOptions {
        current_messages: &preparation.messages_to_summarize,
        models,
        model,
        reserve_tokens: preparation.settings.reserve_tokens,
        signal: options.signal,
        custom_instructions: options.custom_instructions,
        previous_summary: preparation.previous_summary.as_ref(),
        thinking_level: options.thinking_level,
        retry: options.retry,
    }
}

async fn summarize_preparation<C: RetryCallbacks + ?Sized>(
    preparation: &CompactionPreparation,
    models: &dyn Models,
    model: &Model,
    options: &CompactOptions<'_>,
    callbacks: Option<&mut C>,
) -> Result<(JsString, Usage), CompactionError> {
    if preparation.is_split_turn && !preparation.turn_prefix_messages.is_empty() {
        return split_turn_summary(preparation, models, model, options, callbacks).await;
    }
    let history_options = preparation_summary_options(preparation, models, model, options);
    let summary = generate_summary_with_options(&history_options, callbacks).await?;
    Ok((summary.text, summary.usage))
}

async fn split_turn_summary<C: RetryCallbacks + ?Sized>(
    preparation: &CompactionPreparation,
    models: &dyn Models,
    model: &Model,
    options: &CompactOptions<'_>,
    mut callbacks: Option<&mut C>,
) -> Result<(JsString, Usage), CompactionError> {
    let mut history_text = JsString::from("No prior history.");
    let mut history_usage = None;
    if !preparation.messages_to_summarize.is_empty() {
        let history_options = preparation_summary_options(preparation, models, model, options);
        let summary = generate_summary_with_options(&history_options, callbacks.as_deref_mut()).await?;
        history_text = summary.text;
        history_usage = Some(summary.usage);
    }
    let (turn_prefix_text, turn_prefix_usage) = generate_turn_prefix_summary(
        &preparation.turn_prefix_messages,
        models,
        model,
        preparation.settings.reserve_tokens,
        options,
        callbacks,
    )
    .await?;
    let summary = history_text + "\n\n---\n\n**Turn Context (split turn):**\n\n" + turn_prefix_text;
    let usage = match history_usage {
        Some(history_usage) => combine_usage(&history_usage, &turn_prefix_usage),
        None => turn_prefix_usage,
    };
    Ok((summary, usage))
}

async fn generate_turn_prefix_summary<C: RetryCallbacks + ?Sized>(
    messages: &[AgentMessage],
    models: &dyn Models,
    model: &Model,
    reserve_tokens: f64,
    options: &CompactOptions<'_>,
    callbacks: Option<&mut C>,
) -> Result<(JsString, Usage), CompactionError> {
    let max_tokens = output_token_cap(model, 0.5, reserve_tokens);
    let conversation_text = serialize_conversation(&convert_to_llm(messages));
    let prompt_text = JsString::from("<conversation>\n")
        + conversation_text
        + "\n</conversation>\n\n"
        + TURN_PREFIX_SUMMARIZATION_PROMPT;
    let summarization_message = Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(prompt_text))]),
        input_source: None,
        provider_native: None,
        timestamp: now_ms(),
    });
    let completion_options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                signal: options.signal.cloned(),
                ..Default::default()
            },
            max_tokens: Some(max_tokens),
            ..Default::default()
        },
        reasoning: reasoning_option(model, options.thinking_level),
        ..Default::default()
    };
    let context = Context {
        system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.into()),
        messages: vec![summarization_message],
        tools: None,
    };
    let response =
        complete_simple_with_retries(models, model, context, completion_options, options.retry, callbacks).await;
    summarize_response(
        &response,
        "Turn prefix summarization aborted",
        "Turn prefix summarization failed: ",
    )
}
