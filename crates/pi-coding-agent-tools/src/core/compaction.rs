//! Port of `packages/coding-agent/src/core/compaction/{compaction,index}.ts`.
//!
//! Token estimation, cut-point detection and summary generation for coding-agent
//! sessions. The agent package's harness compaction shares the estimation
//! helpers but has a different session-entry model and cut-point algorithm.

pub mod branch_summarization;
pub mod utils;

use std::time::{SystemTime, UNIX_EPOCH};

use pi_agent_core::agent::{AgentMessage, StreamFn};
pub use pi_agent_core::harness::compaction::{ContextUsageEstimate, estimate_context_tokens, estimate_tokens};
use pi_agent_core::harness::messages::{
    CompactionSummaryOptions as MessageCompactionSummaryOptions, convert_to_llm, create_branch_summary_message,
    create_compaction_summary_message, create_custom_message,
};
use pi_ai::compat::complete_simple;
use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, CacheRetention, Context, Message, Model, ProviderHeaders,
    ProviderNativeContext, StopReason, TextContent, TextOrImageContent, Usage, UserContent, UserMessage, UserRole,
};
pub use pi_ai::utils::estimate::calculate_context_tokens;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use serde::{Deserialize, Serialize};

pub use self::utils::{
    FileLists, FileOperations, SUMMARIZATION_SYSTEM_PROMPT, compute_file_lists, create_file_ops,
    extract_file_ops_from_message, format_file_operations, serialize_conversation,
};
use crate::core::messages::is_cut_point_message;
use crate::core::session_manager::{
    KnownSessionEntry, LeafSelection, SessionEntry, build_session_context, session_entry_to_context_messages,
};

/// Details stored in `CompactionEntry.details` for file tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionDetails {
    pub read_files: Vec<JsString>,
    pub modified_files: Vec<JsString>,
}

/// Runtime source used for a compaction, for logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionSourceType {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "openai_remote")]
    OpenAiRemote,
}

/// Runtime source used for logging. Not persisted in session entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionSourceInfo {
    pub r#type: CompactionSourceType,
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<JsString>,
}

/// Result from [`compact`] — the session manager adds uuid/parentUuid when saving.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct CompactionResult<T = Value> {
    pub summary: JsString,
    pub first_kept_entry_id: JsString,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub tokens_before: f64,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub duration_ms: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub estimated_tokens_after: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub kept_from_previous_context_tokens: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub compacted_result_tokens: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub compacted_result_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<CompactionSourceInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_native: Option<ProviderNativeContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<T>,
}

/// Failure surfaced by compaction helpers; mirrors the TypeScript `Error` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionError {
    message: JsString,
}

impl CompactionError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl std::fmt::Display for CompactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}

impl std::error::Error for CompactionError {}

/// Compaction trigger settings.
/// Coding-agent settings include the optional percentage absent from harness settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_percent: Option<f64>,
    pub enabled: bool,
    pub reserve_tokens: f64,
    pub keep_recent_tokens: f64,
}

pub const DEFAULT_COMPACTION_SETTINGS: CompactionSettings = CompactionSettings {
    threshold_percent: None,
    enabled: true,
    reserve_tokens: 16384.0,
    keep_recent_tokens: 20000.0,
};

const GOAL_REMINDER_PREFIXES: [&str; 2] = [
    "Work toward this objective until it is achieved:",
    "Continue working toward this objective until it is achieved:",
];

const MAX_PROTECTED_RECENT_CONTEXT_MESSAGES: usize = 3;
const MAX_PROTECTED_RECENT_CONTEXT_TOKENS: f64 = 10_000.0;

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn extract_file_operations(
    messages: &[AgentMessage],
    entries: &[SessionEntry],
    prev_compaction_index: Option<usize>,
) -> FileOperations {
    let mut file_ops = create_file_ops();
    if let Some(index) = prev_compaction_index
        && let Some(compaction) = entries[index].as_compaction()
        && !compaction.from_hook.unwrap_or(false)
        && let Some(details) = compaction.details.as_deref()
    {
        if let Some(read_files) = details.get("readFiles").and_then(Value::as_array) {
            for path in read_files.iter().filter_map(Value::as_string) {
                file_ops.read.insert(path.to_owned());
            }
        }
        if let Some(modified_files) = details.get("modifiedFiles").and_then(Value::as_array) {
            for path in modified_files.iter().filter_map(Value::as_string) {
                file_ops.edited.insert(path.to_owned());
            }
        }
    }

    for message in messages {
        extract_file_ops_from_message(message, &mut file_ops);
    }

    file_ops
}

/// Extract an [`AgentMessage`] from an entry if it produces one.
fn get_message_from_entry(entry: &SessionEntry) -> Option<AgentMessage> {
    match entry.known().as_deref() {
        Some(KnownSessionEntry::Message(entry)) => Some((*entry.message).clone()),
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
            &MessageCompactionSummaryOptions {
                duration_ms: entry.duration_ms,
                ..MessageCompactionSummaryOptions::default()
            },
        )),
        _ => None,
    }
}

fn get_message_from_entry_for_compaction(entry: &SessionEntry) -> Option<AgentMessage> {
    if matches!(entry.known().as_deref(), Some(KnownSessionEntry::Compaction(_))) {
        return None;
    }
    if let Some(KnownSessionEntry::Message(session_message)) = entry.known().as_deref()
        && let AgentMessage::User(user) = session_message.message.as_ref()
        && user.input_source == Some(pi_ai::types::InputSource::Extension)
    {
        let text = match &user.content {
            UserContent::Text(text) => Some(text),
            UserContent::Blocks(blocks) => blocks.iter().find_map(|block| match block {
                TextOrImageContent::Text(text) => Some(&text.text),
                TextOrImageContent::Image(_) => None,
            }),
        };
        if let Some(text) = text
            && GOAL_REMINDER_PREFIXES.iter().any(|prefix| text.starts_with(*prefix))
        {
            return None;
        }
    }
    get_message_from_entry(entry)
}

/// Return usage from the last valid assistant message in session entries.
pub fn get_last_assistant_usage(entries: &[SessionEntry]) -> Option<Usage> {
    entries.iter().rev().find_map(|entry| {
        let message = entry.as_message()?;
        let assistant = message.message.as_assistant()?;
        if matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error) {
            return None;
        }
        (calculate_context_tokens(&assistant.usage) > 0.0).then(|| assistant.usage.clone())
    })
}

/// Return whether compaction should trigger based on context usage.
pub fn should_compact(
    context_tokens: f64,
    context_window: f64,
    settings: &CompactionSettings,
    auto_compaction_threshold: Option<f64>,
) -> Result<bool, CompactionError> {
    let threshold_value = settings.threshold_percent.map(Value::Number);
    let threshold_percent =
        crate::core::compaction_threshold::parse_compaction_threshold_percent(threshold_value.as_ref())
            .map_err(|failure| CompactionError::new(failure.message().clone()))?;
    if !settings.enabled {
        return Ok(false);
    }
    if let Some(percent) = threshold_percent {
        return Ok(context_tokens >= (context_window * percent) / 100.0);
    }
    if let Some(threshold) = auto_compaction_threshold {
        return Ok(context_tokens >= threshold);
    }
    Ok(context_tokens > context_window - settings.reserve_tokens)
}

fn estimate_entry_tokens(entry: &SessionEntry) -> Option<f64> {
    get_message_from_entry(entry).map(|message| estimate_tokens(&message))
}

fn find_context_message_count_cut_index(entries: &[SessionEntry], start_index: usize, end_index: usize) -> usize {
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

fn find_token_bounded_cut_index(entries: &[SessionEntry], start_index: usize, end_index: usize) -> usize {
    let mut accumulated_tokens = 0.0;
    let mut suffix_start_index = end_index;

    for index in (start_index..end_index).rev() {
        let Some(tokens) = estimate_entry_tokens(&entries[index]) else {
            continue;
        };
        if accumulated_tokens + tokens > MAX_PROTECTED_RECENT_CONTEXT_TOKENS {
            break;
        }
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
        .unwrap_or(index)
}

fn find_protected_suffix_start_index(entries: &[SessionEntry], start_index: usize, end_index: usize) -> usize {
    for index in (start_index..end_index).rev() {
        if entries[index].entry_type() == "compaction" {
            return index + 1;
        }
    }
    start_index
}

fn is_turn_start_entry(entry: &SessionEntry) -> bool {
    if entry.entry_type() == "branch_summary" || entry.entry_type() == "custom_message" {
        return true;
    }
    if entry.entry_type() != "message" {
        return false;
    }
    entry.as_message().is_some_and(|entry| {
        ["user", "bashExecution", "custom"]
            .iter()
            .any(|role| entry.message.has_role(role))
    })
}

fn is_active_turn_continuation_entry(entry: &SessionEntry) -> bool {
    entry.entry_type() == "message"
        && entry
            .as_message()
            .is_some_and(|entry| entry.message.has_role("assistant") || entry.message.has_role("toolResult"))
}

fn find_active_turn_floor_index(entries: &[SessionEntry], start_index: usize, end_index: usize) -> Option<usize> {
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
    entries: &[SessionEntry],
    start_index: usize,
    end_index: usize,
    cut_points: &[usize],
) -> usize {
    let protected_start_index = find_protected_suffix_start_index(entries, start_index, end_index);
    let message_count_cut_index = find_context_message_count_cut_index(entries, protected_start_index, end_index);
    let token_bounded_cut_index = find_token_bounded_cut_index(entries, protected_start_index, end_index);
    let mut shorter_suffix_cut_index = message_count_cut_index.max(token_bounded_cut_index);
    let active_turn_floor_index = find_active_turn_floor_index(entries, protected_start_index, end_index);
    let first_cut_point_index = cut_points.first().copied();
    let has_earlier_compactable_context = active_turn_floor_index
        .zip(first_cut_point_index)
        .is_some_and(|(floor, first)| first < floor);
    if let Some(floor) = active_turn_floor_index
        && has_earlier_compactable_context
    {
        shorter_suffix_cut_index = shorter_suffix_cut_index.min(floor);
    }
    find_nearest_cut_point_at_or_after(cut_points, shorter_suffix_cut_index)
}

fn find_budget_cut_index(
    entries: &[SessionEntry],
    start_index: usize,
    end_index: usize,
    cut_points: &[usize],
    keep_recent_tokens: f64,
) -> usize {
    let mut accumulated_tokens = 0.0;
    for index in (start_index..end_index).rev() {
        let entry_tokens: f64 = session_entry_to_context_messages(&entries[index])
            .iter()
            .map(estimate_tokens)
            .sum();
        if entry_tokens == 0.0 {
            continue;
        }
        accumulated_tokens += entry_tokens;
        if accumulated_tokens >= keep_recent_tokens {
            return find_nearest_cut_point_at_or_after(cut_points, index);
        }
    }
    cut_points[0]
}

fn find_valid_cut_points(entries: &[SessionEntry], start_index: usize, end_index: usize) -> Vec<usize> {
    let mut cut_points = Vec::new();
    for (index, entry) in entries.iter().enumerate().take(end_index).skip(start_index) {
        if entry.entry_type() == "compaction" {
            continue;
        }
        if entry.entry_type() == "custom_message"
            || session_entry_to_context_messages(entry)
                .iter()
                .any(is_cut_point_message)
        {
            cut_points.push(index);
        }
    }
    cut_points
}

/// Find the context-visible turn-start entry containing `entry_index`, or `-1`.
pub fn find_turn_start_index(entries: &[SessionEntry], entry_index: usize, start_index: usize) -> i64 {
    for index in (start_index..=entry_index).rev() {
        if is_turn_start_entry(&entries[index]) {
            return index as i64;
        }
    }
    -1
}

/// Selected cut point for compaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutPointResult {
    /// Index of the first entry to keep.
    pub first_kept_entry_index: usize,
    /// Index of the turn-start entry when the cut splits a turn, otherwise `-1`.
    pub turn_start_index: i64,
    /// Whether the selected cut point splits an in-progress turn.
    pub is_split_turn: bool,
}

fn include_adjacent_metadata(entries: &[SessionEntry], cut_index: usize, start_index: usize) -> usize {
    let mut first_kept_entry_index = cut_index;
    while first_kept_entry_index > start_index {
        let previous_entry = &entries[first_kept_entry_index - 1];
        let crosses_compaction_boundary = previous_entry.entry_type() == "compaction";
        let produces_context = !session_entry_to_context_messages(previous_entry).is_empty();
        if crosses_compaction_boundary || produces_context {
            break;
        }
        first_kept_entry_index -= 1;
    }
    first_kept_entry_index
}

fn describe_cut_point(entries: &[SessionEntry], cut_index: usize, start_index: usize) -> CutPointResult {
    let starts_turn = is_turn_start_entry(&entries[cut_index]);
    let turn_start_index = if starts_turn {
        -1
    } else {
        find_turn_start_index(entries, cut_index, start_index)
    };
    CutPointResult {
        first_kept_entry_index: cut_index,
        turn_start_index,
        is_split_turn: !starts_turn && turn_start_index != -1,
    }
}

/// Find the cut point that keeps approximately `keep_recent_tokens`.
pub fn find_cut_point(
    entries: &[SessionEntry],
    start_index: usize,
    end_index: usize,
    keep_recent_tokens: f64,
) -> CutPointResult {
    let cut_points = find_valid_cut_points(entries, start_index, end_index);
    if cut_points.is_empty() {
        return CutPointResult {
            first_kept_entry_index: start_index,
            turn_start_index: -1,
            is_split_turn: false,
        };
    }

    let budget_cut_index = find_budget_cut_index(entries, start_index, end_index, &cut_points, keep_recent_tokens);
    let protected_cut_index = find_protected_suffix_cut_index(entries, start_index, end_index, &cut_points);
    let selected_cut_index = budget_cut_index.max(protected_cut_index);
    let cut_index = include_adjacent_metadata(entries, selected_cut_index, start_index);
    if cut_index >= end_index {
        return CutPointResult {
            first_kept_entry_index: end_index,
            turn_start_index: -1,
            is_split_turn: false,
        };
    }
    describe_cut_point(entries, cut_index, start_index)
}

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

const COMPACTION_MATERIALIZATION_PROMPT: &str = "Write the complete plaintext continuation summary represented by the preceding compaction checkpoint.

Return only the summary text. Preserve all context needed to continue the work, including goals, constraints, completed progress, in-progress work, decisions, next steps, and critical file or error details.

Do not mention this request, encryption, or provider-native storage.";

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

fn create_summarization_options(
    max_tokens: f64,
    api_key: Option<JsString>,
    headers: Option<ProviderHeaders>,
    env: Option<pi_ai::types::ProviderEnv>,
    signal: Option<AbortSignal>,
) -> SimpleStreamOptions {
    SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                signal,
                api_key,
                headers,
                env,
                ..ProviderRequestOptions::default()
            },
            max_tokens: Some(max_tokens),
            ..StreamOptions::default()
        },
        ..SimpleStreamOptions::default()
    }
}

/// Complete one frozen summary request; acquisition and result rejection escape unchanged.
pub(crate) async fn complete_summarization(
    model: &Model,
    context: Context,
    mut options: SimpleStreamOptions,
    stream_fn: Option<&StreamFn>,
) -> Result<AssistantMessage, pi_agent_core::AgentError> {
    options.stream.cache_retention = Some(CacheRetention::None);
    let result = match stream_fn {
        Some(stream_fn) => stream_fn(model.clone(), context, options)
            .await?
            .result()
            .await
            .map_err(|error| pi_ai::request::CallbackError::new(error.message().clone())),
        None => complete_simple(model.clone(), context, options).await,
    };
    result.map_err(pi_agent_core::AgentError::from)
}

/// Options for generating or updating a conversation summary.
pub struct GenerateSummaryWithUsageOptions<'a> {
    pub current_messages: &'a [AgentMessage],
    pub model: &'a Model,
    pub reserve_tokens: f64,
    pub api_key: Option<JsString>,
    pub headers: Option<ProviderHeaders>,
    pub signal: Option<AbortSignal>,
    pub custom_instructions: Option<&'a JsString>,
    pub previous_summary: Option<&'a JsString>,
    pub thinking_level: Option<pi_agent_core::harness::types::ThinkingLevel>,
    pub stream_fn: Option<&'a StreamFn>,
    pub env: Option<pi_ai::types::ProviderEnv>,
}

/// Result of materializing a provider-native compaction checkpoint into plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeCompactionSummaryResult {
    Aborted,
    Summary(JsString),
}

/// Options for [`materialize_compaction_summary`].
pub struct MaterializeCompactionSummaryOptions<'a> {
    pub reserve_tokens: f64,
    pub api_key: Option<JsString>,
    pub headers: Option<ProviderHeaders>,
    pub env: Option<pi_ai::types::ProviderEnv>,
    pub signal: Option<AbortSignal>,
    pub stream_fn: Option<&'a StreamFn>,
}

fn get_summarization_max_tokens(model: &Model, reserve_tokens: f64) -> f64 {
    let model_limit = if model.max_tokens > 0.0 {
        model.max_tokens
    } else {
        f64::INFINITY
    };
    (0.8 * reserve_tokens).floor().min(model_limit)
}

fn build_compaction_materialization_messages(
    entry: &crate::core::session_manager::CompactionEntry,
) -> Result<Vec<Message>, CompactionError> {
    let checkpoint = create_compaction_summary_message(
        &entry.summary,
        entry.tokens_before,
        &entry.timestamp,
        &MessageCompactionSummaryOptions {
            duration_ms: entry.duration_ms,
            provider_native: entry.provider_native.as_deref().cloned(),
            ..MessageCompactionSummaryOptions::default()
        },
    );
    let request = AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(
            COMPACTION_MATERIALIZATION_PROMPT,
        ))]),
        input_source: None,
        provider_native: None,
        timestamp: pi_js_compat::date::js_now_millis(),
    });
    convert_to_llm(&[checkpoint, request]).map_err(|error| CompactionError::new(error.message()))
}

fn read_compaction_materialization_response(
    response: &AssistantMessage,
) -> Result<MaterializeCompactionSummaryResult, CompactionError> {
    if response.stop_reason == StopReason::Aborted {
        return Ok(MaterializeCompactionSummaryResult::Aborted);
    }
    if response.stop_reason == StopReason::Error {
        let detail = response
            .error_message
            .clone()
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "Unknown provider error".into());
        return Err(CompactionError::new(
            JsString::from("Compaction summary materialization failed: ") + detail,
        ));
    }
    let summary = JsString::join(
        response
            .content
            .iter()
            .filter_map(|content| match content {
                pi_ai::types::AssistantContent::Text(text) => Some(&text.text),
                _ => None,
            })
            .filter(|text| !text.trim().is_empty()),
        "\n",
    );
    if summary.is_empty() {
        return Err(CompactionError::new(
            "Compaction summary materialization returned no text",
        ));
    }
    Ok(MaterializeCompactionSummaryResult::Summary(summary))
}

/// Materialize a provider-native compaction checkpoint without mutating session state.
pub async fn materialize_compaction_summary(
    entry: &crate::core::session_manager::CompactionEntry,
    model: &Model,
    options: &MaterializeCompactionSummaryOptions<'_>,
) -> Result<MaterializeCompactionSummaryResult, CompactionError> {
    if entry.provider_native.is_none() {
        return Err(CompactionError::new(pi_js_compat::js_format!(
            "Compaction entry {} has no provider-native checkpoint",
            &entry.id
        )));
    }
    let response = complete_summarization(
        model,
        Context {
            system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.into()),
            messages: build_compaction_materialization_messages(entry)?,
            tools: None,
        },
        create_summarization_options(
            get_summarization_max_tokens(model, options.reserve_tokens),
            options.api_key.clone(),
            options.headers.clone(),
            options.env.clone(),
            options.signal.clone(),
        ),
        options.stream_fn,
    )
    .await
    .map_err(|error| CompactionError::new(error.message()))?;
    read_compaction_materialization_response(&response)
}

fn build_summary_prompt(options: &GenerateSummaryWithUsageOptions<'_>) -> Result<JsString, CompactionError> {
    let summary_prompt = if options.previous_summary.is_some() {
        UPDATE_SUMMARIZATION_PROMPT
    } else {
        SUMMARIZATION_PROMPT
    };
    let focused_prompt = match options.custom_instructions {
        Some(instructions) => JsString::from(summary_prompt) + "\n\nAdditional focus: " + instructions,
        None => summary_prompt.into(),
    };
    let conversation_text = serialize_conversation(
        &convert_to_llm(options.current_messages).map_err(|error| CompactionError::new(error.message()))?,
    );
    let previous_summary_text = match options.previous_summary {
        Some(summary) => JsString::from("<previous-summary>\n") + summary + "\n</previous-summary>\n\n",
        None => JsString::default(),
    };
    Ok(JsString::from("<conversation>\n")
        + conversation_text
        + "\n</conversation>\n\n"
        + previous_summary_text
        + focused_prompt)
}

fn build_summary_request(
    options: &GenerateSummaryWithUsageOptions<'_>,
) -> Result<(Context, SimpleStreamOptions), CompactionError> {
    let max_tokens = get_summarization_max_tokens(options.model, options.reserve_tokens);
    let message = Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(build_summary_prompt(
            options,
        )?))]),
        input_source: None,
        provider_native: None,
        timestamp: pi_js_compat::date::js_now_millis(),
    });
    let context = Context {
        system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.into()),
        messages: vec![message],
        tools: None,
    };
    Ok((
        context,
        create_summarization_options(
            max_tokens,
            options.api_key.clone(),
            options.headers.clone(),
            options.env.clone(),
            options.signal.clone(),
        ),
    ))
}

/// Summary text and the provider usage that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryWithUsage {
    pub text: JsString,
    pub usage: Usage,
}

async fn generate_summary_with_options(
    options: &GenerateSummaryWithUsageOptions<'_>,
) -> Result<SummaryWithUsage, CompactionError> {
    let (context, completion_options) = build_summary_request(options)?;
    let response = complete_summarization(options.model, context, completion_options, options.stream_fn)
        .await
        .map_err(|error| CompactionError::new(error.message()))?;
    if response.stop_reason == StopReason::Error {
        let detail = response
            .error_message
            .clone()
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "Unknown error".into());
        return Err(CompactionError::new(JsString::from("Summarization failed: ") + detail));
    }
    Ok(SummaryWithUsage {
        text: JsString::join(
            response.content.iter().filter_map(|content| match content {
                AssistantContent::Text(content) if !content.text.trim().is_empty() => Some(&content.text),
                _ => None,
            }),
            "\n",
        ),
        usage: response.usage,
    })
}

/// Generate a summary of the conversation using the LLM.
pub async fn generate_summary(options: &GenerateSummaryWithUsageOptions<'_>) -> Result<JsString, CompactionError> {
    generate_summary_with_options(options).await.map(|summary| summary.text)
}

/// Generate or update a conversation summary and return its provider usage.
pub async fn generate_summary_with_usage(
    options: &GenerateSummaryWithUsageOptions<'_>,
) -> Result<SummaryWithUsage, CompactionError> {
    generate_summary_with_options(options).await
}

/// Data prepared for compaction, before any LLM call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionPreparation {
    /// UUID of the first entry to keep.
    pub first_kept_entry_id: JsString,
    /// Messages that will be summarized and discarded.
    pub messages_to_summarize: Vec<AgentMessage>,
    /// Messages turned into a turn-prefix summary when splitting.
    pub turn_prefix_messages: Vec<AgentMessage>,
    pub is_split_turn: bool,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub tokens_before: f64,
    /// Summary from the previous compaction, for iterative update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_summary: Option<JsString>,
    /// File operations extracted from `messages_to_summarize`.
    pub file_ops: FileOperations,
    pub settings: CompactionSettings,
}

/// Prepare compaction inputs from the active session path.
pub fn prepare_compaction(
    path_entries: &[SessionEntry],
    settings: CompactionSettings,
) -> Option<CompactionPreparation> {
    if path_entries
        .last()
        .is_some_and(|entry| entry.entry_type() == "compaction")
    {
        return None;
    }

    let prev_compaction_index = path_entries
        .iter()
        .rposition(|entry| entry.entry_type() == "compaction");

    let mut previous_summary = None;
    let mut boundary_start = 0;
    if let Some(index) = prev_compaction_index {
        let prev_compaction = path_entries[index].as_compaction()?;
        previous_summary = Some(prev_compaction.summary.clone());
        let first_kept_entry_index = path_entries
            .iter()
            .position(|entry| entry.id() == Some(&prev_compaction.first_kept_entry_id));
        boundary_start = first_kept_entry_index.unwrap_or(index + 1);
    }
    let boundary_end = path_entries.len();

    let tokens_before =
        estimate_context_tokens(&build_session_context(path_entries, LeafSelection::Last).messages).tokens;

    let cut_point = find_cut_point(path_entries, boundary_start, boundary_end, settings.keep_recent_tokens);

    let first_kept_entry = path_entries.get(cut_point.first_kept_entry_index);
    if cut_point.first_kept_entry_index < boundary_end && first_kept_entry.and_then(SessionEntry::id).is_none() {
        return None;
    }
    let first_kept_entry_id = first_kept_entry.and_then(SessionEntry::id).cloned().unwrap_or_default();

    let history_end = if cut_point.is_split_turn {
        cut_point.turn_start_index as usize
    } else {
        cut_point.first_kept_entry_index
    };

    let messages_to_summarize: Vec<AgentMessage> = path_entries[boundary_start..history_end]
        .iter()
        .filter_map(get_message_from_entry_for_compaction)
        .collect();

    let turn_prefix_messages: Vec<AgentMessage> = if cut_point.is_split_turn {
        path_entries[cut_point.turn_start_index as usize..cut_point.first_kept_entry_index]
            .iter()
            .filter_map(get_message_from_entry_for_compaction)
            .collect()
    } else {
        Vec::new()
    };

    if messages_to_summarize.is_empty() && turn_prefix_messages.is_empty() {
        return None;
    }

    let mut file_ops = extract_file_operations(&messages_to_summarize, path_entries, prev_compaction_index);
    if cut_point.is_split_turn {
        for message in &turn_prefix_messages {
            extract_file_ops_from_message(message, &mut file_ops);
        }
    }

    Some(CompactionPreparation {
        first_kept_entry_id,
        messages_to_summarize,
        turn_prefix_messages,
        is_split_turn: cut_point.is_split_turn,
        tokens_before,
        previous_summary,
        file_ops,
        settings,
    })
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
        cost: pi_ai::types::UsageCost {
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

/// Options shared by the summary calls inside [`compact`].
pub struct CompactionSummaryOptions<'a> {
    pub model: &'a Model,
    pub api_key: Option<JsString>,
    pub headers: Option<ProviderHeaders>,
    pub custom_instructions: Option<&'a JsString>,
    pub signal: Option<AbortSignal>,
    pub thinking_level: Option<pi_agent_core::harness::types::ThinkingLevel>,
    pub stream_fn: Option<&'a StreamFn>,
    pub env: Option<pi_ai::types::ProviderEnv>,
}

fn build_history_summary_options<'a>(
    preparation: &'a CompactionPreparation,
    options: &'a CompactionSummaryOptions<'a>,
) -> GenerateSummaryWithUsageOptions<'a> {
    GenerateSummaryWithUsageOptions {
        current_messages: &preparation.messages_to_summarize,
        model: options.model,
        reserve_tokens: preparation.settings.reserve_tokens,
        api_key: options.api_key.clone(),
        headers: options.headers.clone(),
        signal: options.signal.clone(),
        custom_instructions: options.custom_instructions,
        previous_summary: preparation.previous_summary.as_ref(),
        thinking_level: options.thinking_level,
        stream_fn: options.stream_fn,
        env: options.env.clone(),
    }
}

struct TurnPrefixSummaryRequestOptions<'a> {
    messages: &'a [AgentMessage],
    model: &'a Model,
    reserve_tokens: f64,
    api_key: Option<JsString>,
    headers: Option<ProviderHeaders>,
    env: Option<pi_ai::types::ProviderEnv>,
    signal: Option<AbortSignal>,
}

fn build_turn_prefix_summary_request(
    options: &TurnPrefixSummaryRequestOptions<'_>,
) -> Result<(Context, SimpleStreamOptions), CompactionError> {
    let max_tokens = (0.5 * options.reserve_tokens)
        .floor()
        .min(if options.model.max_tokens > 0.0 {
            options.model.max_tokens
        } else {
            f64::INFINITY
        });
    let conversation_text = serialize_conversation(
        &convert_to_llm(options.messages).map_err(|error| CompactionError::new(error.message()))?,
    );
    let prompt_text = JsString::from("<conversation>\n")
        + conversation_text
        + "\n</conversation>\n\n"
        + TURN_PREFIX_SUMMARIZATION_PROMPT;
    let message = Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(prompt_text))]),
        input_source: None,
        provider_native: None,
        timestamp: pi_js_compat::date::js_now_millis(),
    });
    Ok((
        Context {
            system_prompt: Some(SUMMARIZATION_SYSTEM_PROMPT.into()),
            messages: vec![message],
            tools: None,
        },
        create_summarization_options(
            max_tokens,
            options.api_key.clone(),
            options.headers.clone(),
            options.env.clone(),
            options.signal.clone(),
        ),
    ))
}

fn interpret_turn_prefix_summary_response(response: &AssistantMessage) -> Result<SummaryWithUsage, CompactionError> {
    if response.stop_reason == StopReason::Error {
        let detail = response
            .error_message
            .clone()
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "Unknown error".into());
        return Err(CompactionError::new(
            JsString::from("Turn prefix summarization failed: ") + detail,
        ));
    }
    Ok(SummaryWithUsage {
        text: JsString::join(
            response.content.iter().filter_map(|content| match content {
                AssistantContent::Text(content) if !content.text.trim().is_empty() => Some(&content.text),
                _ => None,
            }),
            "\n",
        ),
        usage: response.usage.clone(),
    })
}

async fn generate_turn_prefix_summary(
    options: &TurnPrefixSummaryRequestOptions<'_>,
    stream_fn: Option<&StreamFn>,
) -> Result<SummaryWithUsage, CompactionError> {
    let (context, request_options) = build_turn_prefix_summary_request(options)?;
    let response = complete_summarization(options.model, context, request_options, stream_fn)
        .await
        .map_err(|error| CompactionError::new(error.message()))?;
    interpret_turn_prefix_summary_response(&response)
}

async fn generate_split_turn_summary(
    preparation: &CompactionPreparation,
    options: &CompactionSummaryOptions<'_>,
) -> Result<SummaryWithUsage, CompactionError> {
    let mut history_text = JsString::from("No prior history.");
    let mut history_usage = None;

    if !preparation.messages_to_summarize.is_empty() {
        let history_result =
            generate_summary_with_options(&build_history_summary_options(preparation, options)).await?;
        history_text = history_result.text;
        history_usage = Some(history_result.usage);
    }

    let turn_prefix_result = generate_turn_prefix_summary(
        &TurnPrefixSummaryRequestOptions {
            messages: &preparation.turn_prefix_messages,
            model: options.model,
            reserve_tokens: preparation.settings.reserve_tokens,
            api_key: options.api_key.clone(),
            headers: options.headers.clone(),
            env: options.env.clone(),
            signal: options.signal.clone(),
        },
        options.stream_fn,
    )
    .await?;
    let text = history_text + "\n\n---\n\n**Turn Context (split turn):**\n\n" + turn_prefix_result.text;
    let usage = match history_usage {
        Some(usage) => combine_usage(&usage, &turn_prefix_result.usage),
        None => turn_prefix_result.usage,
    };
    Ok(SummaryWithUsage { text, usage })
}

async fn generate_compaction_summary(
    preparation: &CompactionPreparation,
    options: &CompactionSummaryOptions<'_>,
) -> Result<SummaryWithUsage, CompactionError> {
    if preparation.is_split_turn && !preparation.turn_prefix_messages.is_empty() {
        return generate_split_turn_summary(preparation, options).await;
    }
    generate_summary_with_options(&build_history_summary_options(preparation, options)).await
}

/// Generate summaries for compaction using prepared data.
pub async fn compact(
    preparation: &CompactionPreparation,
    model: &Model,
    options: &CompactionSummaryOptions<'_>,
) -> Result<CompactionResult, CompactionError> {
    let started_at = now_millis();
    let summary_result = generate_compaction_summary(preparation, options).await?;
    let file_lists = compute_file_lists(&preparation.file_ops);
    let summary = summary_result.text + format_file_operations(&file_lists.read_files, &file_lists.modified_files);

    if preparation.first_kept_entry_id.is_empty() {
        return Err(CompactionError::new(
            "First kept entry has no UUID - session may need migration",
        ));
    }
    Ok(CompactionResult {
        summary,
        first_kept_entry_id: preparation.first_kept_entry_id.clone(),
        tokens_before: preparation.tokens_before,
        duration_ms: Some((now_millis() - started_at) as f64),
        estimated_tokens_after: None,
        kept_from_previous_context_tokens: None,
        compacted_result_tokens: None,
        compacted_result_bytes: None,
        source: Some(CompactionSourceInfo {
            r#type: CompactionSourceType::Local,
            provider: model.provider.clone(),
            model: model.id.clone(),
            endpoint: None,
        }),
        usage: Some(summary_result.usage),
        provider_native: None,
        details: Some(
            pi_js_compat::js_json::to_value(&CompactionDetails {
                read_files: file_lists.read_files,
                modified_files: file_lists.modified_files,
            })
            .unwrap_or(Value::Null),
        ),
    })
}

pub use branch_summarization::{
    BranchPreparation, BranchSummaryDetails, BranchSummaryResult, CollectEntriesResult, GenerateBranchSummaryOptions,
    collect_entries_for_branch_summary, generate_branch_summary, prepare_branch_entries,
};
