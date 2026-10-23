//! Frozen agent-core/harness/compaction/compaction.ts.

use self::utils::{compute_file_lists, create_file_ops, extract_file_ops_from_message, format_file_operations};
use crate::agent::AgentMessage;
use crate::harness::messages::{
    CompactionSummaryOptions, convert_to_llm, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message,
};
use crate::harness::session::{KnownSessionEntry, SessionTreeEntry as Entry, build_session_context};
use crate::harness::types::{CompactionError, CompactionErrorCode, ThinkingLevel};
use pi_ai::models::Models;
use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, Context, Message, Model, StopReason, TextContent, TextOrImageContent, Usage,
    UserContent, UserMessage, UserRole,
};
use pi_ai::utils::estimate::{
    TextOrImageInput, estimate_assistant_message_tokens, estimate_text_and_image_content_tokens,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};
use serde::{Deserialize, Serialize};

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

/// Return usage from the last valid assistant message in tree entries.
pub fn get_last_assistant_usage(entries: &[Entry]) -> Option<&Usage> {
    entries
        .iter()
        .rev()
        .filter_map(Entry::message)
        .find_map(get_assistant_usage)
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

pub(super) fn get_message_from_entry(entry: &Entry) -> Option<AgentMessage> {
    match entry.known()? {
        KnownSessionEntry::Message { message, .. } => Some(message.clone()),
        KnownSessionEntry::CustomMessage {
            custom_type,
            content,
            display,
            details,
            ..
        } => Some(create_custom_message(
            custom_type,
            content.clone(),
            *display,
            details.clone(),
            entry.timestamp(),
        )),
        KnownSessionEntry::BranchSummary { summary, from_id, .. } => {
            Some(create_branch_summary_message(summary, from_id, entry.timestamp()))
        }
        KnownSessionEntry::Compaction {
            summary,
            tokens_before,
            duration_ms,
            ..
        } => Some(create_compaction_summary_message(
            summary,
            *tokens_before,
            entry.timestamp(),
            &CompactionSummaryOptions {
                duration_ms: *duration_ms,
                ..Default::default()
            },
        )),
        _ => None,
    }
}
fn get_message_from_entry_for_compaction(entry: &Entry) -> Option<AgentMessage> {
    if entry.kind() == "compaction" {
        None
    } else {
        get_message_from_entry(entry)
    }
}
fn estimate_entry_tokens(entry: &Entry) -> Option<f64> {
    get_message_from_entry(entry).map(|message| estimate_tokens(&message))
}
fn is_turn_start_entry(entry: &Entry) -> bool {
    entry.kind() == "branch_summary"
        || entry.kind() == "custom_message"
        || entry.message().is_some_and(|message| {
            ["user", "bashExecution", "custom"]
                .iter()
                .any(|role| message.has_role(role))
        })
}
fn is_active_turn_continuation_entry(entry: &Entry) -> bool {
    entry
        .message()
        .is_some_and(|message| message.has_role("assistant") || message.has_role("toolResult"))
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
        if entries[index].kind() == "compaction" {
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
        if let Some(message) = entry.message()
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
        if entry.kind() == "branch_summary" || entry.kind() == "custom_message" {
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
        if ["compaction", "message", "branch_summary", "custom_message"]
            .iter()
            .any(|kind| previous.kind() == *kind)
        {
            break;
        }
        cut_index -= 1;
    }

    let cut_entry = &entries[cut_index];
    let is_user_message = cut_entry.message().is_some_and(|message| message.has_role("user"));
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

#[derive(Debug, Clone)]
pub struct CompactionPreparation {
    pub first_kept_entry_id: JsString,
    pub messages_to_summarize: Vec<AgentMessage>,
    pub turn_prefix_messages: Vec<AgentMessage>,
    pub is_split_turn: bool,
    pub tokens_before: f64,
    pub previous_summary: Option<JsString>,
    pub file_ops: FileOperations,
    pub settings: CompactionSettings,
}

fn extract_file_operations(messages: &[AgentMessage], entries: &[Entry], previous: Option<usize>) -> FileOperations {
    let mut file_ops = create_file_ops();
    if let Some(index) = previous
        && let Some(KnownSessionEntry::Compaction {
            details: Some(details),
            from_hook,
            ..
        }) = entries[index].known()
        && *from_hook != Some(true)
    {
        merge_file_details(&mut file_ops, details);
    }
    for message in messages {
        extract_file_ops_from_message(message, &mut file_ops);
    }
    file_ops
}
pub(super) fn merge_file_details(file_ops: &mut FileOperations, details: &Value) {
    if let Some(read) = details.get("readFiles").and_then(Value::as_array) {
        file_ops.read.extend(read.iter().filter_map(Value::as_string).cloned());
    }
    if let Some(modified) = details.get("modifiedFiles").and_then(Value::as_array) {
        file_ops
            .edited
            .extend(modified.iter().filter_map(Value::as_string).cloned());
    }
}

pub fn prepare_compaction(
    path_entries: &[Entry],
    settings: &CompactionSettings,
) -> Result<Option<CompactionPreparation>, CompactionError> {
    if path_entries.is_empty() || path_entries.last().is_some_and(|entry| entry.kind() == "compaction") {
        return Ok(None);
    }
    let previous = path_entries.iter().rposition(|entry| entry.kind() == "compaction");
    let (previous_summary, boundary_start) = if let Some(index) = previous {
        let Some(KnownSessionEntry::Compaction {
            summary,
            first_kept_entry_id,
            ..
        }) = path_entries[index].known()
        else {
            return Err(CompactionError::new(
                CompactionErrorCode::InvalidSession,
                "Invalid compaction entry payload",
                None,
            ));
        };
        (
            Some(summary.clone()),
            path_entries
                .iter()
                .position(|entry| entry.id() == first_kept_entry_id)
                .unwrap_or(index + 1),
        )
    } else {
        (None, 0)
    };
    let context = build_session_context(path_entries).map_err(|error| {
        CompactionError::new(
            CompactionErrorCode::InvalidSession,
            error.message.clone(),
            Some(Box::new(error)),
        )
    })?;
    let tokens_before = estimate_context_tokens(&context.messages).tokens;
    let cut = find_cut_point(
        path_entries,
        boundary_start,
        path_entries.len(),
        settings.keep_recent_tokens,
    );
    let kept_index = cut.first_kept_entry_index as usize;
    let first_kept_entry_id = path_entries
        .get(kept_index)
        .map(Entry::id)
        .filter(|id| !id.is_empty())
        .cloned()
        .ok_or_else(missing_kept_entry)?;
    let history_end = if cut.is_split_turn {
        cut.turn_start_index as usize
    } else {
        kept_index
    };
    let messages_to_summarize = path_entries[boundary_start..history_end]
        .iter()
        .filter_map(get_message_from_entry_for_compaction)
        .collect::<Vec<_>>();
    let turn_prefix_messages = if cut.is_split_turn {
        path_entries[history_end..kept_index]
            .iter()
            .filter_map(get_message_from_entry_for_compaction)
            .collect()
    } else {
        Vec::new()
    };
    if messages_to_summarize.is_empty() && turn_prefix_messages.is_empty() {
        return Ok(None);
    }
    let mut file_ops = extract_file_operations(&messages_to_summarize, path_entries, previous);
    if cut.is_split_turn {
        for message in &turn_prefix_messages {
            extract_file_ops_from_message(message, &mut file_ops);
        }
    }
    Ok(Some(CompactionPreparation {
        first_kept_entry_id,
        messages_to_summarize,
        turn_prefix_messages,
        is_split_turn: cut.is_split_turn,
        tokens_before,
        previous_summary,
        file_ops,
        settings: *settings,
    }))
}
fn missing_kept_entry() -> CompactionError {
    CompactionError::new(
        CompactionErrorCode::InvalidSession,
        "First kept entry has no UUID - session may need migration",
        None,
    )
}

#[derive(Clone, Copy)]
pub struct GenerateSummaryOptions<'a> {
    pub current_messages: &'a [AgentMessage],
    pub models: &'a dyn Models,
    pub model: &'a Model,
    pub reserve_tokens: f64,
    pub signal: Option<&'a AbortSignal>,
    pub custom_instructions: Option<&'a JsString>,
    pub previous_summary: Option<&'a JsString>,
    pub thinking_level: Option<ThinkingLevel>,
}
#[derive(Debug, Clone)]
pub struct CompactResult {
    pub summary: JsString,
    pub first_kept_entry_id: JsString,
    pub tokens_before: f64,
    pub duration_ms: Option<f64>,
    pub details: Option<CompactionDetails>,
}
#[derive(Clone, Copy, Default)]
pub struct CompactOptions<'a> {
    pub custom_instructions: Option<&'a JsString>,
    pub signal: Option<&'a AbortSignal>,
    pub thinking_level: Option<ThinkingLevel>,
}

pub(super) fn now_ms() -> f64 {
    pi_js_compat::date::js_now_millis()
}
fn build_summary_prompt(options: &GenerateSummaryOptions<'_>) -> Result<JsString, CompactionError> {
    let previous = options.previous_summary.filter(|summary| !summary.is_empty());
    let mut prompt = if previous.is_some() {
        JsString::from(UPDATE_SUMMARIZATION_PROMPT)
    } else {
        SUMMARIZATION_PROMPT.into()
    };
    if let Some(instructions) = options.custom_instructions.filter(|text| !text.is_empty()) {
        prompt = prompt + "\n\nAdditional focus: " + instructions;
    }
    let text = serialize_conversation(&convert_to_llm(options.current_messages).map_err(message_conversion_failure)?)
        .slice(0, MAX_COMPACTION_CONTEXT_CHARS);
    let mut result = JsString::from("<conversation>\n") + text + "\n</conversation>\n\n";
    if let Some(previous) = previous {
        result = result + "<previous-summary>\n" + previous + "\n</previous-summary>\n\n";
    }
    Ok(result + prompt)
}
fn output_token_cap(model: &Model, fraction: f64, reserve: f64) -> f64 {
    let requested = (fraction * reserve).floor();
    let cap = if model.max_tokens > 0.0 {
        model.max_tokens
    } else {
        f64::INFINITY
    };
    if requested.is_nan() {
        f64::NAN
    } else {
        requested.min(cap)
    }
}
pub(super) async fn complete_summary(
    models: &dyn Models,
    model: &Model,
    prompt: JsString,
    max_tokens: f64,
    signal: Option<&AbortSignal>,
) -> Result<AssistantMessage, CompactionError> {
    let context = Context {
        system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.into()),
        messages: vec![Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(prompt))]),
            input_source: None,
            provider_native: None,
            timestamp: now_ms(),
        })],
        tools: None,
    };
    let options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                signal: signal.cloned(),
                ..Default::default()
            },
            max_tokens: Some(max_tokens),
            ..Default::default()
        },
        ..Default::default()
    };
    models
        .complete_simple(model.clone(), context, options)
        .await
        .map_err(|error| {
            CompactionError::new(
                CompactionErrorCode::Unknown,
                error.message().clone(),
                Some(Box::new(error)),
            )
        })
}
pub(super) fn response_text(response: &AssistantMessage) -> JsString {
    JsString::join(
        response.content.iter().filter_map(|block| match block {
            AssistantContent::Text(text) => Some(text.text.clone()),
            _ => None,
        }),
        "\n",
    )
}
fn summarize_response(response: AssistantMessage, aborted: &str, prefix: &str) -> Result<JsString, CompactionError> {
    if response.stop_reason == StopReason::Aborted {
        return Err(CompactionError::new(
            CompactionErrorCode::Aborted,
            response
                .error_message
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| aborted.into()),
            None,
        ));
    }
    if response.stop_reason == StopReason::Error {
        return Err(CompactionError::new(
            CompactionErrorCode::SummarizationFailed,
            JsString::from(prefix)
                + response
                    .error_message
                    .filter(|text| !text.is_empty())
                    .unwrap_or_else(|| "Unknown error".into()),
            None,
        ));
    }
    Ok(response_text(&response))
}
pub async fn generate_summary(options: &GenerateSummaryOptions<'_>) -> Result<JsString, CompactionError> {
    let response = complete_summary(
        options.models,
        options.model,
        build_summary_prompt(options)?,
        output_token_cap(options.model, 0.8, options.reserve_tokens),
        options.signal,
    )
    .await?;
    summarize_response(response, "Summarization aborted", "Summarization failed: ")
}
pub async fn compact(
    preparation: &CompactionPreparation,
    models: &dyn Models,
    model: &Model,
    options: &CompactOptions<'_>,
) -> Result<CompactResult, CompactionError> {
    let started = now_ms();
    if preparation.first_kept_entry_id.is_empty() {
        return Err(missing_kept_entry());
    }
    let history_options = GenerateSummaryOptions {
        current_messages: &preparation.messages_to_summarize,
        models,
        model,
        reserve_tokens: preparation.settings.reserve_tokens,
        signal: options.signal,
        custom_instructions: options.custom_instructions,
        previous_summary: preparation.previous_summary.as_ref(),
        thinking_level: options.thinking_level,
    };
    let summary = if preparation.is_split_turn && !preparation.turn_prefix_messages.is_empty() {
        let history = if preparation.messages_to_summarize.is_empty() {
            JsString::from("No prior history.")
        } else {
            generate_summary(&history_options).await?
        };
        let text = serialize_conversation(
            &convert_to_llm(&preparation.turn_prefix_messages).map_err(message_conversion_failure)?,
        )
        .slice(0, MAX_COMPACTION_CONTEXT_CHARS);
        let prompt =
            JsString::from("<conversation>\n") + text + "\n</conversation>\n\n" + TURN_PREFIX_SUMMARIZATION_PROMPT;
        let response = complete_summary(
            models,
            model,
            prompt,
            output_token_cap(model, 0.5, preparation.settings.reserve_tokens),
            options.signal,
        )
        .await?;
        history
            + "\n\n---\n\n**Turn Context (split turn):**\n\n"
            + summarize_response(
                response,
                "Turn prefix summarization aborted",
                "Turn prefix summarization failed: ",
            )?
    } else {
        generate_summary(&history_options).await?
    };
    let FileLists {
        read_files,
        modified_files,
    } = compute_file_lists(&preparation.file_ops);
    Ok(CompactResult {
        summary: summary + format_file_operations(&read_files, &modified_files),
        first_kept_entry_id: preparation.first_kept_entry_id.clone(),
        tokens_before: preparation.tokens_before,
        duration_ms: Some(now_ms() - started),
        details: Some(CompactionDetails {
            read_files,
            modified_files,
        }),
    })
}
fn message_conversion_failure(error: crate::agent::AgentError) -> CompactionError {
    CompactionError::new(
        CompactionErrorCode::Unknown,
        error.message().clone(),
        Some(Box::new(error)),
    )
}
