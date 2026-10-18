//! Port of `packages/ai/src/api/openai-responses-shared.ts`.
//!
//! Message/tool conversion and the Responses SSE state machine. Request bodies
//! are built as JSON values to preserve the exact TypeScript wire shape.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use indexmap::IndexMap;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::api::constrained_sampling::{
    GrammarToolInputJsonBuffer, append_grammar_tool_input_json_delta, get_grammar_tool_input,
    get_json_schema_tool_parameters, resolve_grammar_constrained_sampling, resolve_json_schema_strict_sampling,
};
use crate::api::transform_messages::transform_messages_with;
use crate::models::calculate_cost;
use crate::request::BoxFuture;
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, ImageContent, Message, Model,
    ModelCompat, ProviderNativeFormat, StopReason, TextContent, TextSignatureV1, TextSignatureVersion, ThinkingContent,
    Tool, ToolCall, Usage, UsageCost,
};
use crate::utils::event_stream::AssistantMessageEventSender;
use crate::utils::hash::short_hash;
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::sanitize_unicode::sanitize_surrogates;

const RAW_ANNOTATION_START: char = '\u{e200}';
const RAW_ANNOTATION_END: char = '\u{e201}';
const RAW_ANNOTATION_SEPARATOR: char = '\u{e202}';
const RAW_CITATION_PREFIX: &str = "\u{e200}cite\u{e202}";
const NO_SPACE_BEFORE: [char; 9] = ['.', ',', ';', ':', '!', '?', ')', ']', '}'];
const NO_SPACE_AFTER: [char; 3] = ['(', '[', '{'];

pub type ResolveServiceTier = Arc<dyn Fn(Option<&str>, Option<&str>) -> Option<String> + Send + Sync>;
pub type ApplyServiceTierPricing = Arc<dyn Fn(&mut Usage, Option<&str>) + Send + Sync>;

/// `OpenAIResponsesStreamOptions`.
#[derive(Clone, Default)]
pub struct OpenAIResponsesStreamOptions {
    pub service_tier: Option<String>,
    pub grammar_tool_input_properties: Option<IndexMap<String, String>>,
    pub resolve_service_tier: Option<ResolveServiceTier>,
    pub apply_service_tier_pricing: Option<ApplyServiceTierPricing>,
}

/// `ConvertResponsesMessagesOptions`.
#[derive(Clone, Default)]
pub struct ConvertResponsesMessagesOptions {
    pub include_system_prompt: Option<bool>,
    pub grammar_tool_input_properties: Option<IndexMap<String, String>>,
    pub deferred_tools: Option<BTreeMap<String, Tool>>,
    pub deferred_tools_mode: Option<ResponsesDeferredToolsMode>,
    pub tool_options: Option<ConvertResponsesToolsOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsesDeferredToolsMode {
    AdditionalTools,
    ToolSearch,
}

/// `ConvertResponsesToolsOptions`.
#[derive(Clone, Default)]
pub struct ConvertResponsesToolsOptions {
    /// Outer `None` = absent, `Some(None)` = explicit `null`.
    pub strict: Option<Option<bool>>,
    pub supports_strict_mode: Option<bool>,
    pub supports_openai_grammar_tools: Option<bool>,
    pub defer_loading: Option<bool>,
}

/// Failure while consuming stream events: either the event source failed or
/// the state machine rejected an event.
#[derive(Debug)]
pub enum ResponsesStreamFailure<E> {
    Source(E),
    Message(String),
}

/// Async event source for [`process_responses_stream`].
pub trait ResponsesEventSource {
    type Error;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<Value>, Self::Error>>;
}

/// In-memory event source used by tests and replay harnesses.
pub struct VecResponsesEventSource {
    events: std::vec::IntoIter<Value>,
}

impl VecResponsesEventSource {
    pub fn new(events: Vec<Value>) -> Self {
        Self {
            events: events.into_iter(),
        }
    }
}

impl ResponsesEventSource for VecResponsesEventSource {
    type Error = std::convert::Infallible;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<Value>, Self::Error>> {
        Box::pin(async move { Ok(self.events.next()) })
    }
}

// =============================================================================
// Utilities
// =============================================================================

fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key.to_owned(), value);
    }
    Value::Object(map)
}

fn js_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) => "null".to_owned(),
        Some(other) => other.to_string(),
        None => "undefined".to_owned(),
    }
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn encode_text_signature_v1(id: &str, phase: Option<&str>) -> String {
    let payload = TextSignatureV1 {
        v: TextSignatureVersion,
        id: id.to_owned(),
        phase: phase.map(|phase| match phase {
            "commentary" => crate::types::TextPhase::Commentary,
            _ => crate::types::TextPhase::FinalAnswer,
        }),
    };
    serde_json::to_string(&payload).unwrap_or_default()
}

struct ParsedTextSignature {
    id: String,
    phase: Option<String>,
}

fn parse_text_signature(signature: Option<&str>) -> Option<ParsedTextSignature> {
    let signature = signature?;
    if signature.starts_with('{')
        && let Ok(Value::Object(parsed)) = serde_json::from_str::<Value>(signature)
        && parsed.get("v").and_then(Value::as_u64) == Some(1)
        && let Some(id) = parsed.get("id").and_then(Value::as_str)
    {
        let phase = match parsed.get("phase").and_then(Value::as_str) {
            Some(phase @ ("commentary" | "final_answer")) => Some(phase.to_owned()),
            _ => None,
        };
        return Some(ParsedTextSignature {
            id: id.to_owned(),
            phase,
        });
    }
    Some(ParsedTextSignature {
        id: signature.to_owned(),
        phase: None,
    })
}

fn convert_tool_result_output(model: &Model, content: &[crate::types::TextOrImageContent]) -> Value {
    let text_result = content
        .iter()
        .filter_map(|block| match block {
            crate::types::TextOrImageContent::Text(text) => Some(text.text.as_str()),
            crate::types::TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let images = content
        .iter()
        .filter_map(|block| match block {
            crate::types::TextOrImageContent::Image(image) => Some(image),
            crate::types::TextOrImageContent::Text(_) => None,
        })
        .collect::<Vec<_>>();
    let has_text = !text_result.is_empty();

    if images.is_empty() || !model.input.contains(&crate::types::Modality::Image) {
        let text = if has_text {
            text_result
        } else if images.is_empty() {
            "(no tool output)".to_owned()
        } else {
            "(see attached image)".to_owned()
        };
        return Value::String(sanitize_surrogates(&text).to_owned());
    }

    let mut output = Vec::new();
    if has_text {
        output.push(object(vec![
            ("type", Value::String("input_text".to_owned())),
            ("text", Value::String(sanitize_surrogates(&text_result).to_owned())),
        ]));
    }
    for image in images {
        output.push(object(vec![
            ("type", Value::String("input_image".to_owned())),
            ("detail", Value::String("auto".to_owned())),
            (
                "image_url",
                Value::String(format!("data:{};base64,{}", image.mime_type, image.data)),
            ),
        ]));
    }
    Value::Array(output)
}

// =============================================================================
// Hosted-search citation normalization
// =============================================================================

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CitationTextState {
    marker_buffer: String,
    pending_whitespace: String,
    removed_citation: bool,
    last_character: Option<char>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedCitationText {
    state: CitationTextState,
    text: String,
}

fn create_citation_text_state(last_character: Option<char>) -> CitationTextState {
    CitationTextState {
        last_character,
        ..CitationTextState::default()
    }
}

fn is_raw_hosted_search_citation(marker: &str) -> bool {
    if !marker.starts_with(RAW_CITATION_PREFIX) || !marker.ends_with(RAW_ANNOTATION_END) {
        return false;
    }
    let payload = &marker[RAW_CITATION_PREFIX.len()..marker.len() - RAW_ANNOTATION_END.len_utf8()];
    let references = payload.split(RAW_ANNOTATION_SEPARATOR).collect::<Vec<_>>();
    !references.is_empty() && references.iter().all(|reference| is_citation_reference(reference))
}

fn is_citation_reference(reference: &str) -> bool {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| regex::Regex::new(r"^turn[0-9]+[A-Za-z][A-Za-z0-9_-]*[0-9]+$").expect("citation regex"))
        .is_match(reference)
}

fn get_citation_boundary_separator(last_character: Option<char>, next_character: char) -> &'static str {
    let Some(last_character) = last_character else {
        return "";
    };
    if last_character.is_whitespace() || next_character.is_whitespace() {
        return "";
    }
    if NO_SPACE_AFTER.contains(&last_character) || NO_SPACE_BEFORE.contains(&next_character) {
        return "";
    }
    " "
}

fn append_visible_citation_text(state: &CitationTextState, text: &str) -> NormalizedCitationText {
    let mut pending_whitespace = state.pending_whitespace.clone();
    let mut removed_citation = state.removed_citation;
    let mut last_character = state.last_character;
    let mut normalized_text = String::new();

    for character in text.chars() {
        if character == ' ' || character == '\t' {
            pending_whitespace.push(character);
            continue;
        }
        if removed_citation {
            normalized_text.push_str(get_citation_boundary_separator(last_character, character));
        } else {
            normalized_text.push_str(&pending_whitespace);
        }
        normalized_text.push(character);
        pending_whitespace.clear();
        removed_citation = false;
        last_character = Some(character);
    }

    NormalizedCitationText {
        state: CitationTextState {
            marker_buffer: state.marker_buffer.clone(),
            pending_whitespace,
            removed_citation,
            last_character,
        },
        text: normalized_text,
    }
}

fn find_trailing_citation_buffer_start(text: &str) -> Option<usize> {
    let marker_start_index = text.rfind(RAW_ANNOTATION_START)?;
    let marker_candidate = &text[marker_start_index..];
    if marker_candidate.contains(RAW_ANNOTATION_END) {
        return None;
    }
    let could_be_citation =
        RAW_CITATION_PREFIX.starts_with(marker_candidate) || marker_candidate.starts_with(RAW_CITATION_PREFIX);
    could_be_citation.then_some(marker_start_index)
}

fn normalize_closed_citation_markers(state: &CitationTextState, text: &str) -> NormalizedCitationText {
    let mut current_state = state.clone();
    let mut normalized_text = String::new();
    let mut text_offset = 0;

    while let Some(relative_start) = text[text_offset..].find(RAW_ANNOTATION_START) {
        let marker_start_index = text_offset + relative_start;
        let search_start = marker_start_index + RAW_ANNOTATION_START.len_utf8();
        let Some(relative_end) = text[search_start..].find(RAW_ANNOTATION_END) else {
            break;
        };
        let marker_end = search_start + relative_end + RAW_ANNOTATION_END.len_utf8();

        let visible = append_visible_citation_text(&current_state, &text[text_offset..marker_start_index]);
        current_state = visible.state;
        normalized_text.push_str(&visible.text);

        let marker = &text[marker_start_index..marker_end];
        if is_raw_hosted_search_citation(marker) {
            current_state.removed_citation = true;
        } else {
            let preserved = append_visible_citation_text(&current_state, marker);
            current_state = preserved.state;
            normalized_text.push_str(&preserved.text);
        }
        text_offset = marker_end;
    }

    let trailing = append_visible_citation_text(&current_state, &text[text_offset..]);
    NormalizedCitationText {
        state: trailing.state,
        text: normalized_text + &trailing.text,
    }
}

fn normalize_citation_text_delta(state: &CitationTextState, delta: &str) -> NormalizedCitationText {
    let combined_text = format!("{}{}", state.marker_buffer, delta);
    let marker_buffer_start = find_trailing_citation_buffer_start(&combined_text);
    let (stable_text, marker_buffer) = match marker_buffer_start {
        Some(index) => (combined_text[..index].to_owned(), combined_text[index..].to_owned()),
        None => (combined_text, String::new()),
    };
    let mut without_buffer = state.clone();
    without_buffer.marker_buffer.clear();
    let normalized = normalize_closed_citation_markers(&without_buffer, &stable_text);
    let mut next_state = normalized.state;
    next_state.marker_buffer = marker_buffer;
    NormalizedCitationText {
        state: next_state,
        text: normalized.text,
    }
}

fn finish_citation_text(state: &CitationTextState) -> NormalizedCitationText {
    let mut current_state = state.clone();
    current_state.marker_buffer.clear();
    let mut normalized_text = String::new();

    if !state.marker_buffer.is_empty() {
        let preserved = append_visible_citation_text(&current_state, &state.marker_buffer);
        current_state = preserved.state;
        normalized_text = preserved.text;
    }

    if current_state.removed_citation {
        current_state.pending_whitespace.clear();
        current_state.removed_citation = false;
        return NormalizedCitationText {
            state: current_state,
            text: normalized_text,
        };
    }

    let pending = std::mem::take(&mut current_state.pending_whitespace);
    NormalizedCitationText {
        state: current_state,
        text: normalized_text + &pending,
    }
}

fn normalize_citation_text(text: &str) -> String {
    let normalized = normalize_citation_text_delta(&create_citation_text_state(None), text);
    normalized.text + &finish_citation_text(&normalized.state).text
}

// =============================================================================
// Message conversion
// =============================================================================

fn normalize_id_part(part: &str) -> String {
    let sanitized: String = part
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let normalized: String = sanitized.chars().take(64).collect();
    normalized.trim_end_matches('_').to_owned()
}

fn build_foreign_responses_item_id(item_id: &str) -> String {
    let normalized = format!("fc_{}", short_hash(item_id));
    normalized.chars().take(64).collect()
}

fn split_tool_call_id(id: &str) -> (&str, Option<&str>) {
    match id.split_once('|') {
        Some((call_id, item_id)) => (call_id, Some(item_id)),
        None => (id, None),
    }
}

pub fn convert_responses_messages(
    model: &Model,
    context: &Context,
    allowed_tool_call_providers: &BTreeSet<String>,
    options: Option<&ConvertResponsesMessagesOptions>,
) -> Result<Vec<Value>, String> {
    let empty_options = ConvertResponsesMessagesOptions::default();
    let options = options.unwrap_or(&empty_options);
    let mut messages = Vec::new();
    let mut loaded_tool_names = BTreeSet::new();

    let normalize_tool_call_id = |id: &str, _target: &Model, source: &AssistantMessage| -> String {
        if !allowed_tool_call_providers.contains(&model.provider) {
            return normalize_id_part(id);
        }
        let Some((call_id, item_id)) = id.split_once('|') else {
            return normalize_id_part(id);
        };
        let normalized_call_id = normalize_id_part(call_id);
        let is_foreign = source.provider != model.provider || source.api != model.api;
        let mut normalized_item_id = if is_foreign {
            build_foreign_responses_item_id(item_id)
        } else {
            normalize_id_part(item_id)
        };
        if normalized_item_id.starts_with("ctc_") {
            return format!("{normalized_call_id}|{normalized_item_id}");
        }
        if !normalized_item_id.starts_with("fc_") {
            normalized_item_id = normalize_id_part(&format!("fc_{normalized_item_id}"));
        }
        format!("{normalized_call_id}|{normalized_item_id}")
    };

    let transformed_messages = transform_messages_with(
        &context.messages,
        model,
        Some(&normalize_tool_call_id),
        crate::request::now_millis(),
    );

    let include_system_prompt = options.include_system_prompt.unwrap_or(true);
    if include_system_prompt && let Some(system_prompt) = context.system_prompt.as_deref() {
        let supports_developer_role = match &model.compat {
            Some(ModelCompat::OpenAIResponses(compat)) => compat.supports_developer_role != Some(false),
            _ => true,
        };
        let role = if model.reasoning && supports_developer_role {
            "developer"
        } else {
            "system"
        };
        messages.push(object(vec![
            ("role", Value::String(role.to_owned())),
            ("content", Value::String(sanitize_surrogates(system_prompt).to_owned())),
        ]));
    }

    let mut msg_index = 0;
    let mut emitted_tool_call_ids = BTreeSet::new();
    for msg in &transformed_messages {
        match msg {
            Message::User(user) => {
                if let Some(native) = &user.provider_native
                    && native.provider == model.provider
                    && native.api == model.api
                    && native.format == ProviderNativeFormat::OpenAIResponsesInput
                    && let Value::Array(items) = &native.value
                {
                    messages.extend(items.iter().cloned());
                    continue;
                }

                match &user.content {
                    crate::types::UserContent::Text(text) => {
                        messages.push(object(vec![
                            ("role", Value::String("user".to_owned())),
                            (
                                "content",
                                Value::Array(vec![object(vec![
                                    ("type", Value::String("input_text".to_owned())),
                                    ("text", Value::String(sanitize_surrogates(text).to_owned())),
                                ])]),
                            ),
                        ]));
                    }
                    crate::types::UserContent::Blocks(blocks) => {
                        let content = blocks
                            .iter()
                            .map(|block| match block {
                                crate::types::TextOrImageContent::Text(text) => object(vec![
                                    ("type", Value::String("input_text".to_owned())),
                                    ("text", Value::String(sanitize_surrogates(&text.text).to_owned())),
                                ]),
                                crate::types::TextOrImageContent::Image(image) => object(vec![
                                    ("type", Value::String("input_image".to_owned())),
                                    ("detail", Value::String("auto".to_owned())),
                                    (
                                        "image_url",
                                        Value::String(format!("data:{};base64,{}", image.mime_type, image.data)),
                                    ),
                                ]),
                            })
                            .collect::<Vec<_>>();
                        if content.is_empty() {
                            continue;
                        }
                        messages.push(object(vec![
                            ("role", Value::String("user".to_owned())),
                            ("content", Value::Array(content)),
                        ]));
                    }
                }
            }
            Message::Assistant(assistant) => {
                let mut output = Vec::new();
                let is_same_provider_and_api = assistant.provider == model.provider && assistant.api == model.api;
                let is_same_model = is_same_provider_and_api && assistant.model == model.id;
                let is_different_model = is_same_provider_and_api && assistant.model != model.id;
                let mut text_block_index = 0;

                for block in &assistant.content {
                    match block {
                        AssistantContent::Thinking(thinking) => {
                            if let Some(signature) = thinking.thinking_signature.as_deref()
                                && let Ok(item) = serde_json::from_str::<Value>(signature)
                            {
                                output.push(item);
                            }
                        }
                        AssistantContent::Text(text) => {
                            let parsed_signature = parse_text_signature(text.text_signature.as_deref());
                            let fallback_message_id = if text_block_index == 0 {
                                format!("msg_pi_{msg_index}")
                            } else {
                                format!("msg_pi_{msg_index}_{text_block_index}")
                            };
                            text_block_index += 1;
                            let msg_id = match parsed_signature.as_ref() {
                                None => fallback_message_id,
                                Some(parsed) if parsed.id.is_empty() => fallback_message_id,
                                Some(parsed) if parsed.id.len() > 64 => format!("msg_{}", short_hash(&parsed.id)),
                                Some(parsed) => parsed.id.clone(),
                            };
                            let mut message = object(vec![
                                ("type", Value::String("message".to_owned())),
                                ("role", Value::String("assistant".to_owned())),
                                (
                                    "content",
                                    Value::Array(vec![object(vec![
                                        ("type", Value::String("output_text".to_owned())),
                                        ("text", Value::String(sanitize_surrogates(&text.text).to_owned())),
                                        ("annotations", Value::Array(Vec::new())),
                                    ])]),
                                ),
                                ("status", Value::String("completed".to_owned())),
                                ("id", Value::String(msg_id)),
                            ]);
                            if let Some(phase) = parsed_signature.and_then(|parsed| parsed.phase)
                                && let Value::Object(map) = &mut message
                            {
                                map.insert("phase".to_owned(), Value::String(phase));
                            }
                            output.push(message);
                        }
                        AssistantContent::ToolCall(tool_call) => {
                            let (call_id, item_id_raw) = split_tool_call_id(&tool_call.id);
                            emitted_tool_call_ids.insert(call_id.to_owned());
                            let custom_input_property = options
                                .grammar_tool_input_properties
                                .as_ref()
                                .and_then(|properties| properties.get(&tool_call.name))
                                .cloned();
                            let mut item_id = item_id_raw.map(str::to_owned);
                            let is_fc_item = item_id.as_deref().is_some_and(|id| id.starts_with("fc_"));
                            if (is_different_model && is_fc_item) || (custom_input_property.is_none() && !is_fc_item) {
                                item_id = None;
                            }

                            let can_replay_namespace = is_same_model
                                || options
                                    .deferred_tools
                                    .as_ref()
                                    .is_some_and(|tools| tools.contains_key(&tool_call.name));

                            let mut entries = if let Some(property) = custom_input_property.as_deref() {
                                let input = get_grammar_tool_input(&tool_call.name, &tool_call.arguments, property)
                                    .map_err(|error| error.message)?;
                                vec![
                                    ("type", Value::String("custom_tool_call".to_owned())),
                                    ("id", item_id.map(Value::String).unwrap_or(Value::Null)),
                                    ("call_id", Value::String(call_id.to_owned())),
                                    ("name", Value::String(tool_call.name.clone())),
                                    ("input", Value::String(sanitize_surrogates(&input).to_owned())),
                                ]
                            } else {
                                vec![
                                    ("type", Value::String("function_call".to_owned())),
                                    ("id", item_id.map(Value::String).unwrap_or(Value::Null)),
                                    ("call_id", Value::String(call_id.to_owned())),
                                    ("name", Value::String(tool_call.name.clone())),
                                    (
                                        "arguments",
                                        Value::String(serde_json::to_string(&tool_call.arguments).unwrap_or_default()),
                                    ),
                                ]
                            };
                            if let (true, Some(namespace)) = (can_replay_namespace, tool_call.namespace.as_ref()) {
                                entries.push(("namespace", Value::String(namespace.clone())));
                            }
                            let mut item = object(entries);
                            if let Value::Object(map) = &mut item
                                && map.get("id") == Some(&Value::Null)
                            {
                                map.shift_remove("id");
                            }
                            output.push(item);
                        }
                    }
                }
                if output.is_empty() {
                    continue;
                }
                messages.extend(output);
            }
            Message::ToolResult(result) => {
                let (call_id, _) = split_tool_call_id(&result.tool_call_id);
                if !emitted_tool_call_ids.contains(call_id) {
                    continue;
                }
                let output = convert_tool_result_output(model, &result.content);

                let is_custom = options
                    .grammar_tool_input_properties
                    .as_ref()
                    .is_some_and(|properties| properties.contains_key(&result.tool_name));
                messages.push(object(vec![
                    (
                        "type",
                        Value::String(if is_custom {
                            "custom_tool_call_output".to_owned()
                        } else {
                            "function_call_output".to_owned()
                        }),
                    ),
                    ("call_id", Value::String(call_id.to_owned())),
                    ("output", output),
                ]));

                let mut deferred_tools = Vec::new();
                for name in result.added_tool_names.iter().flatten() {
                    let Some(tool) = options.deferred_tools.as_ref().and_then(|tools| tools.get(name)) else {
                        continue;
                    };
                    if !loaded_tool_names.insert(name.clone()) {
                        continue;
                    }
                    deferred_tools.push(tool.clone());
                }
                match options.deferred_tools_mode {
                    Some(ResponsesDeferredToolsMode::AdditionalTools) if !deferred_tools.is_empty() => {
                        messages.push(object(vec![
                            ("type", Value::String("additional_tools".to_owned())),
                            ("role", Value::String("developer".to_owned())),
                            (
                                "tools",
                                convert_responses_tools(&deferred_tools, options.tool_options.as_ref())
                                    .map_err(|error| error.message)?,
                            ),
                        ]));
                    }
                    Some(ResponsesDeferredToolsMode::ToolSearch) if !deferred_tools.is_empty() => {
                        let names = deferred_tools.iter().map(|tool| tool.name.clone()).collect::<Vec<_>>();
                        let search_call_id = format!(
                            "pi_tool_load_{}",
                            short_hash(&format!("{}:{}", result.tool_call_id, names.join(",")))
                        );
                        messages.push(object(vec![
                            ("type", Value::String("tool_search_call".to_owned())),
                            ("call_id", Value::String(search_call_id.clone())),
                            ("execution", Value::String("client".to_owned())),
                            ("status", Value::String("completed".to_owned())),
                            (
                                "arguments",
                                object(vec![
                                    ("query", Value::String(names.join(" "))),
                                    ("limit", Value::from(names.len())),
                                ]),
                            ),
                        ]));
                        let mut tool_options = options.tool_options.clone().unwrap_or_default();
                        tool_options.defer_loading = Some(true);
                        messages.push(object(vec![
                            ("type", Value::String("tool_search_output".to_owned())),
                            ("call_id", Value::String(search_call_id)),
                            ("execution", Value::String("client".to_owned())),
                            ("status", Value::String("completed".to_owned())),
                            (
                                "tools",
                                convert_responses_tools(&deferred_tools, Some(&tool_options))
                                    .map_err(|error| error.message)?,
                            ),
                        ]));
                    }
                    _ => {}
                }
            }
        }
        msg_index += 1;
    }

    Ok(messages)
}

// =============================================================================
// Tool conversion
// =============================================================================

pub fn convert_responses_tools(
    tools: &[Tool],
    options: Option<&ConvertResponsesToolsOptions>,
) -> Result<Value, crate::api::constrained_sampling::ConstrainedSamplingError> {
    let empty_options = ConvertResponsesToolsOptions::default();
    let options = options.unwrap_or(&empty_options);
    let default_strict = match options.strict {
        None => Some(false),
        Some(strict) => strict,
    };
    let supports_strict_mode = options.supports_strict_mode.unwrap_or(true);
    let supports_openai_grammar_tools = options.supports_openai_grammar_tools.unwrap_or(false);

    let mut converted = Vec::new();
    for tool in tools {
        if let Some(grammar) = resolve_grammar_constrained_sampling(tool, supports_openai_grammar_tools)? {
            let mut entries = vec![
                ("type", Value::String("custom".to_owned())),
                ("name", Value::String(tool.name.clone())),
                ("description", Value::String(tool.description.clone())),
                (
                    "format",
                    object(vec![
                        ("type", Value::String("grammar".to_owned())),
                        ("syntax", serde_json::to_value(grammar.format).unwrap_or(Value::Null)),
                        ("definition", Value::String(grammar.definition)),
                    ]),
                ),
            ];
            if options.defer_loading == Some(true) {
                entries.push(("defer_loading", Value::Bool(true)));
            }
            converted.push(object(entries));
            continue;
        }

        let constrained_strict = resolve_json_schema_strict_sampling(tool, supports_strict_mode)?;
        let strict = constrained_strict.or(default_strict);
        let mut entries = vec![
            ("type", Value::String("function".to_owned())),
            ("name", Value::String(tool.name.clone())),
            ("description", Value::String(tool.description.clone())),
            ("parameters", get_json_schema_tool_parameters(tool, strict)?),
        ];
        if options.defer_loading == Some(true) {
            entries.push(("defer_loading", Value::Bool(true)));
        }
        if supports_strict_mode {
            entries.push(("strict", strict.map(Value::Bool).unwrap_or(Value::Null)));
        }
        converted.push(object(entries));
    }
    Ok(Value::Array(converted))
}

// =============================================================================
// Stream processing
// =============================================================================

#[derive(Default, Clone)]
struct CustomInput {
    property: String,
    json_buffer: GrammarToolInputJsonBuffer,
}

enum Slot {
    Thinking {
        content_index: usize,
    },
    Text {
        content_index: usize,
        citation_state: Option<CitationTextState>,
    },
    ToolCall {
        content_index: usize,
        partial_json: Option<String>,
        custom_input: Option<CustomInput>,
    },
}

fn get_slot(slots: &BTreeMap<Option<u64>, Slot>, output_index: Option<u64>, kind: &str) -> Option<usize> {
    let slot = slots.get(&output_index)?;
    let matches = matches!(
        (slot, kind),
        (Slot::Thinking { .. }, "thinking") | (Slot::Text { .. }, "text") | (Slot::ToolCall { .. }, "toolCall")
    );
    matches.then_some(match slot {
        Slot::Thinking { content_index } | Slot::Text { content_index, .. } | Slot::ToolCall { content_index, .. } => {
            *content_index
        }
    })
}

fn custom_tool_call_input(block: &ToolCall, custom_input: &Option<CustomInput>) -> String {
    let Some(custom_input) = custom_input else {
        return String::new();
    };
    block
        .arguments
        .get(&custom_input.property)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn append_custom_tool_call_input(
    block: &mut ToolCall,
    custom_input: &mut Option<CustomInput>,
    next_input: &str,
    close: bool,
) -> Result<Option<String>, String> {
    let Some(custom_input) = custom_input.as_mut() else {
        return Ok(None);
    };
    let delta =
        append_grammar_tool_input_json_delta(&mut custom_input.json_buffer, &custom_input.property, next_input, close)
            .map_err(|error| error.message)?;
    block
        .arguments
        .insert(custom_input.property.clone(), Value::String(next_input.to_owned()));
    Ok(delta)
}

fn push_tool_call_delta(
    sender: &AssistantMessageEventSender,
    content_index: usize,
    delta: Option<String>,
    output: &AssistantMessage,
) {
    let Some(delta) = delta else {
        return;
    };
    sender.push(AssistantMessageEvent::ToolCallDelta {
        content_index,
        delta,
        partial: output.clone(),
    });
}

fn parse_arguments(partial_json: Option<&str>) -> Map<String, Value> {
    match parse_streaming_json(partial_json) {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn apply_message_phase_stop_reason(output: &mut AssistantMessage, item: &Value) {
    if item.get("type").and_then(Value::as_str) == Some("message")
        && item.get("phase").and_then(Value::as_str) == Some("final_answer")
    {
        output.stop_reason = StopReason::Stop;
    }
}

fn create_slot(
    slots: &mut BTreeMap<Option<u64>, Slot>,
    output_index: Option<u64>,
    item: &Value,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    options: Option<&OpenAIResponsesStreamOptions>,
) -> Option<()> {
    match item.get("type").and_then(Value::as_str) {
        Some("reasoning") => {
            output
                .content
                .push(AssistantContent::Thinking(ThinkingContent::new("")));
            let content_index = output.content.len() - 1;
            slots.insert(output_index, Slot::Thinking { content_index });
            sender.push(AssistantMessageEvent::ThinkingStart {
                content_index,
                partial: output.clone(),
            });
            Some(())
        }
        Some("message") => {
            apply_message_phase_stop_reason(output, item);
            output.content.push(AssistantContent::Text(TextContent::new("")));
            let content_index = output.content.len() - 1;
            slots.insert(
                output_index,
                Slot::Text {
                    content_index,
                    citation_state: None,
                },
            );
            sender.push(AssistantMessageEvent::TextStart {
                content_index,
                partial: output.clone(),
            });
            Some(())
        }
        Some("function_call") => {
            let call_id = js_string(item.get("call_id"));
            let item_id = js_string(item.get("id"));
            let mut block = ToolCall::new(
                format!("{call_id}|{item_id}"),
                item.get("name").and_then(Value::as_str).unwrap_or(""),
                Map::new(),
            );
            if let Some(namespace) = item.get("namespace").and_then(Value::as_str) {
                block.namespace = Some(namespace.to_owned());
            }
            let partial_json = item
                .get("arguments")
                .and_then(Value::as_str)
                .filter(|arguments| !arguments.is_empty())
                .unwrap_or("")
                .to_owned();
            output.content.push(AssistantContent::ToolCall(block));
            let content_index = output.content.len() - 1;
            slots.insert(
                output_index,
                Slot::ToolCall {
                    content_index,
                    partial_json: Some(partial_json),
                    custom_input: None,
                },
            );
            sender.push(AssistantMessageEvent::ToolCallStart {
                content_index,
                partial: output.clone(),
            });
            Some(())
        }
        Some("custom_tool_call") => {
            let call_id = js_string(item.get("call_id"));
            let item_id = js_string(item.get("id"));
            let name = item.get("name").and_then(Value::as_str).unwrap_or("");
            let input_property = options
                .and_then(|options| options.grammar_tool_input_properties.as_ref())
                .and_then(|properties| properties.get(name))
                .cloned()
                .unwrap_or_else(|| "input".to_owned());
            let input = item
                .get("input")
                .and_then(Value::as_str)
                .filter(|input| !input.is_empty())
                .unwrap_or("")
                .to_owned();
            let mut block = ToolCall::new(format!("{call_id}|{item_id}"), name, {
                let mut arguments = Map::new();
                arguments.insert(input_property.clone(), Value::String(input));
                arguments
            });
            if let Some(namespace) = item.get("namespace").and_then(Value::as_str) {
                block.namespace = Some(namespace.to_owned());
            }
            output.content.push(AssistantContent::ToolCall(block));
            let content_index = output.content.len() - 1;
            slots.insert(
                output_index,
                Slot::ToolCall {
                    content_index,
                    partial_json: None,
                    custom_input: Some(CustomInput {
                        property: input_property,
                        json_buffer: GrammarToolInputJsonBuffer::default(),
                    }),
                },
            );
            sender.push(AssistantMessageEvent::ToolCallStart {
                content_index,
                partial: output.clone(),
            });
            Some(())
        }
        _ => None,
    }
}

fn get_or_create_slot(
    slots: &mut BTreeMap<Option<u64>, Slot>,
    output_index: Option<u64>,
    item: &Value,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    options: Option<&OpenAIResponsesStreamOptions>,
) -> Option<()> {
    if slots.contains_key(&output_index) {
        return Some(());
    }
    create_slot(slots, output_index, item, output, sender, options)
}

fn backfill_reasoning_signatures(
    response_output: &[Value],
    reasoning_blocks_by_id: &BTreeMap<String, usize>,
    output: &mut AssistantMessage,
) {
    for item in response_output {
        if item.get("type").and_then(Value::as_str) != Some("reasoning") {
            continue;
        }
        let Some(encrypted_content) = item.get("encrypted_content").filter(|value| is_truthy(value)) else {
            continue;
        };
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(content_index) = reasoning_blocks_by_id.get(id) else {
            continue;
        };
        let AssistantContent::Thinking(block) = &mut output.content[*content_index] else {
            continue;
        };
        let Some(signature) = block.thinking_signature.as_deref() else {
            continue;
        };
        let Ok(Value::Object(mut stored)) = serde_json::from_str::<Value>(signature) else {
            continue;
        };
        if stored.get("encrypted_content").is_some_and(is_truthy) {
            continue;
        }
        stored.insert("encrypted_content".to_owned(), encrypted_content.clone());
        block.thinking_signature = Some(Value::Object(stored).to_string());
    }
}

fn emit_text_delta(
    sender: &AssistantMessageEventSender,
    content_index: usize,
    delta: &str,
    output: &mut AssistantMessage,
) {
    if delta.is_empty() {
        return;
    }
    if let AssistantContent::Text(block) = &mut output.content[content_index] {
        block.text.push_str(delta);
    }
    sender.push(AssistantMessageEvent::TextDelta {
        content_index,
        delta: delta.to_owned(),
        partial: output.clone(),
    });
}

fn append_provider_text_delta(
    sender: &AssistantMessageEventSender,
    content_index: usize,
    delta: &str,
    saw_hosted_web_search_call: bool,
    citation_state: &mut Option<CitationTextState>,
    output: &mut AssistantMessage,
) {
    if !saw_hosted_web_search_call {
        emit_text_delta(sender, content_index, delta, output);
        return;
    }
    let last_character = match &output.content[content_index] {
        AssistantContent::Text(block) => block.text.chars().last(),
        _ => None,
    };
    let state = citation_state
        .clone()
        .unwrap_or_else(|| create_citation_text_state(last_character));
    let normalized = normalize_citation_text_delta(&state, delta);
    *citation_state = Some(normalized.state);
    emit_text_delta(sender, content_index, &normalized.text, output);
}

fn finalize_response(
    response: &Value,
    output: &mut AssistantMessage,
    model: &Model,
    options: Option<&OpenAIResponsesStreamOptions>,
    reasoning_blocks_by_id: &BTreeMap<String, usize>,
) -> Result<(), String> {
    let empty = Vec::new();
    let response_output = response.get("output").and_then(Value::as_array).unwrap_or(&empty);
    backfill_reasoning_signatures(response_output, reasoning_blocks_by_id, output);

    if let Some(id) = response.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()) {
        output.response_id = Some(id.to_owned());
    }

    let image_generation_item = response_output.iter().find(|item| {
        item.get("type").and_then(Value::as_str) == Some("image_generation_call")
            && item.get("status").and_then(Value::as_str) == Some("completed")
            && item.get("result").is_some_and(is_truthy)
    });
    if let Some(item) = image_generation_item
        && let Some(result) = item.get("result").and_then(Value::as_str)
    {
        output.image_generation_result = Some(ImageContent::new(result, "image/png"));
    }

    if let Some(usage) = response.get("usage").filter(|usage| is_truthy(usage)) {
        let input_details = usage.get("input_tokens_details");
        let cached_tokens = input_details
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let cache_write_tokens = input_details
            .and_then(|details| details.get("cache_write_tokens"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let input_tokens = usage.get("input_tokens").and_then(Value::as_f64).unwrap_or(0.0);
        output.usage = Usage {
            input: (input_tokens - cached_tokens - cache_write_tokens).max(0.0),
            output: usage.get("output_tokens").and_then(Value::as_f64).unwrap_or(0.0),
            cache_read: cached_tokens,
            cache_write: cache_write_tokens,
            cache_write_1h: None,
            reasoning: usage
                .get("output_tokens_details")
                .and_then(|details| details.get("reasoning_tokens"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .into(),
            total_tokens: usage.get("total_tokens").and_then(Value::as_f64).unwrap_or(0.0),
            cost: UsageCost::default(),
        };
        output.usage.cost = calculate_cost(model, &output.usage);

        let response_service_tier = response.get("service_tier").and_then(Value::as_str);
        if let Some(options) = options {
            let service_tier = match &options.resolve_service_tier {
                Some(resolve) => resolve(response_service_tier, options.service_tier.as_deref()),
                None => response_service_tier
                    .map(str::to_owned)
                    .or_else(|| options.service_tier.clone()),
            };
            if let Some(apply) = &options.apply_service_tier_pricing {
                apply(&mut output.usage, service_tier.as_deref());
            }
        }
    }

    let status = response.get("status").and_then(Value::as_str);
    let incomplete_reason = response
        .get("incomplete_details")
        .and_then(|details| details.get("reason"))
        .and_then(Value::as_str);
    output.raw_stop_reason = match (status, incomplete_reason) {
        (Some(status), Some(reason)) => Some(format!("{status}.{reason}")),
        (Some(status), None) => Some(status.to_owned()),
        (None, Some(reason)) => Some(format!("undefined.{reason}")),
        (None, None) => None,
    };
    let mapped = map_stop_reason(status, incomplete_reason)?;
    output.stop_reason = mapped.stop_reason;
    output.error_message = mapped.error_message;
    if output
        .content
        .iter()
        .any(|block| matches!(block, AssistantContent::ToolCall(_)))
        && output.stop_reason == StopReason::Stop
    {
        output.stop_reason = StopReason::ToolUse;
    }
    Ok(())
}

struct MappedStopReason {
    stop_reason: StopReason,
    error_message: Option<String>,
}

fn map_stop_reason(status: Option<&str>, incomplete_reason: Option<&str>) -> Result<MappedStopReason, String> {
    let Some(status) = status else {
        return Ok(MappedStopReason {
            stop_reason: StopReason::Stop,
            error_message: None,
        });
    };
    Ok(match status {
        "completed" => MappedStopReason {
            stop_reason: StopReason::Stop,
            error_message: None,
        },
        "incomplete" => match incomplete_reason {
            Some("max_output_tokens") => MappedStopReason {
                stop_reason: StopReason::Length,
                error_message: None,
            },
            Some(reason) => MappedStopReason {
                stop_reason: StopReason::Error,
                error_message: Some(format!("Response incomplete: {reason}")),
            },
            None => MappedStopReason {
                stop_reason: StopReason::Error,
                error_message: Some("Response incomplete without a provider reason".to_owned()),
            },
        },
        "failed" | "cancelled" => MappedStopReason {
            stop_reason: StopReason::Error,
            error_message: None,
        },
        "in_progress" | "queued" => MappedStopReason {
            stop_reason: StopReason::Stop,
            error_message: None,
        },
        other => {
            return Err(format!("Unhandled stop reason: {other}"));
        }
    })
}

fn done_reason(stop_reason: StopReason) -> Option<DoneReason> {
    match stop_reason {
        StopReason::Stop => Some(DoneReason::Stop),
        StopReason::Length => Some(DoneReason::Length),
        StopReason::ToolUse => Some(DoneReason::ToolUse),
        StopReason::Deferred => Some(DoneReason::Deferred),
        StopReason::Pending | StopReason::Error | StopReason::Aborted => None,
    }
}

pub async fn process_responses_stream<S: ResponsesEventSource>(
    source: &mut S,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    model: &Model,
    options: Option<&OpenAIResponsesStreamOptions>,
) -> Result<(), ResponsesStreamFailure<S::Error>> {
    let mut saw_terminal_response_event = false;
    let mut saw_hosted_web_search_call = false;
    let mut output_slots: BTreeMap<Option<u64>, Slot> = BTreeMap::new();
    let mut reasoning_blocks_by_id: BTreeMap<String, usize> = BTreeMap::new();

    loop {
        let event = match source.next_event().await {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(error) => return Err(ResponsesStreamFailure::Source(error)),
        };
        let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
        let output_index = event.get("output_index").and_then(Value::as_u64);

        match event_type {
            "response.created" => {
                if let Some(id) = event
                    .get("response")
                    .and_then(|response| response.get("id"))
                    .and_then(Value::as_str)
                {
                    output.response_id = Some(id.to_owned());
                }
            }
            "response.output_item.added" => {
                let item = event.get("item").cloned().unwrap_or(Value::Null);
                if item.get("type").and_then(Value::as_str) == Some("web_search_call") {
                    saw_hosted_web_search_call = true;
                }
                create_slot(&mut output_slots, output_index, &item, output, sender, options);
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "thinking") else {
                    continue;
                };
                let delta = event.get("delta").and_then(Value::as_str).unwrap_or("");
                if let AssistantContent::Thinking(block) = &mut output.content[content_index] {
                    block.thinking.push_str(delta);
                }
                sender.push(AssistantMessageEvent::ThinkingDelta {
                    content_index,
                    delta: delta.to_owned(),
                    partial: output.clone(),
                });
            }
            "response.reasoning_summary_part.done" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "thinking") else {
                    continue;
                };
                if let AssistantContent::Thinking(block) = &mut output.content[content_index] {
                    block.thinking.push_str("\n\n");
                }
                sender.push(AssistantMessageEvent::ThinkingDelta {
                    content_index,
                    delta: "\n\n".to_owned(),
                    partial: output.clone(),
                });
            }
            "response.output_text.delta" | "response.refusal.delta" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "text") else {
                    continue;
                };
                let delta = event.get("delta").and_then(Value::as_str).unwrap_or("");
                let mut citation_state = match output_slots.get(&output_index) {
                    Some(Slot::Text { citation_state, .. }) => citation_state.clone(),
                    _ => None,
                };
                append_provider_text_delta(
                    sender,
                    content_index,
                    delta,
                    saw_hosted_web_search_call,
                    &mut citation_state,
                    output,
                );
                if let Some(Slot::Text {
                    citation_state: slot_state,
                    ..
                }) = output_slots.get_mut(&output_index)
                {
                    *slot_state = citation_state;
                }
            }
            "response.function_call_arguments.delta" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "toolCall") else {
                    continue;
                };
                let delta = event.get("delta").and_then(Value::as_str).unwrap_or("");
                let Some(Slot::ToolCall { partial_json, .. }) = output_slots.get_mut(&output_index) else {
                    continue;
                };
                let Some(partial_json) = partial_json.as_mut() else {
                    continue;
                };
                partial_json.push_str(delta);
                let parsed = parse_arguments(Some(partial_json));
                if let AssistantContent::ToolCall(block) = &mut output.content[content_index] {
                    block.arguments = parsed;
                }
                push_tool_call_delta(sender, content_index, Some(delta.to_owned()), output);
            }
            "response.function_call_arguments.done" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "toolCall") else {
                    continue;
                };
                let arguments = event.get("arguments").and_then(Value::as_str).unwrap_or("").to_owned();
                let Some(Slot::ToolCall { partial_json, .. }) = output_slots.get_mut(&output_index) else {
                    continue;
                };
                let Some(previous) = partial_json.as_ref() else {
                    continue;
                };
                let delta = arguments
                    .strip_prefix(previous.as_str())
                    .filter(|delta| !delta.is_empty())
                    .map(str::to_owned);
                *partial_json = Some(arguments.clone());
                let parsed = parse_arguments(Some(&arguments));
                if let AssistantContent::ToolCall(block) = &mut output.content[content_index] {
                    block.arguments = parsed;
                }
                push_tool_call_delta(sender, content_index, delta, output);
            }
            "response.custom_tool_call_input.delta" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "toolCall") else {
                    continue;
                };
                let delta = event.get("delta").and_then(Value::as_str).unwrap_or("");
                let Some(Slot::ToolCall { custom_input, .. }) = output_slots.get_mut(&output_index) else {
                    continue;
                };
                if custom_input.is_none() {
                    continue;
                }
                let AssistantContent::ToolCall(block) = &mut output.content[content_index] else {
                    continue;
                };
                let current = custom_tool_call_input(block, custom_input);
                let next_input = current + delta;
                let appended = append_custom_tool_call_input(block, custom_input, &next_input, false);
                match appended {
                    Ok(delta) => push_tool_call_delta(sender, content_index, delta, output),
                    Err(error) => return Err(ResponsesStreamFailure::Message(error)),
                }
            }
            "response.custom_tool_call_input.done" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "toolCall") else {
                    continue;
                };
                let input = event.get("input").and_then(Value::as_str);
                let Some(Slot::ToolCall { custom_input, .. }) = output_slots.get_mut(&output_index) else {
                    continue;
                };
                if custom_input.is_none() {
                    continue;
                }
                let AssistantContent::ToolCall(block) = &mut output.content[content_index] else {
                    continue;
                };
                let next_input = input
                    .map(str::to_owned)
                    .unwrap_or_else(|| custom_tool_call_input(block, custom_input));
                let appended = append_custom_tool_call_input(block, custom_input, &next_input, true);
                match appended {
                    Ok(delta) => push_tool_call_delta(sender, content_index, delta, output),
                    Err(error) => return Err(ResponsesStreamFailure::Message(error)),
                }
            }
            "response.image_generation_call.partial_image" => {
                output.image_generation_result = Some(ImageContent::new(
                    event.get("partial_image_b64").and_then(Value::as_str).unwrap_or(""),
                    "image/png",
                ));
            }
            "response.output_item.done" => {
                let item = event.get("item").cloned().unwrap_or(Value::Null);
                apply_message_phase_stop_reason(output, &item);
                if item.get("type").and_then(Value::as_str) == Some("web_search_call") {
                    saw_hosted_web_search_call = true;
                }
                get_or_create_slot(&mut output_slots, output_index, &item, output, sender, options);
                let slot_kind = match output_slots.get(&output_index) {
                    Some(Slot::Thinking { .. }) => "thinking",
                    Some(Slot::Text { .. }) => "text",
                    Some(Slot::ToolCall { .. }) => "toolCall",
                    None => "",
                };
                let content_index = get_slot(&output_slots, output_index, slot_kind);

                match item.get("type").and_then(Value::as_str) {
                    Some("image_generation_call")
                        if item.get("status").and_then(Value::as_str) == Some("completed")
                            && item.get("result").is_some_and(is_truthy) =>
                    {
                        output.image_generation_result = Some(ImageContent::new(
                            item.get("result").and_then(Value::as_str).unwrap_or(""),
                            "image/png",
                        ));
                    }
                    Some("reasoning") if slot_kind == "thinking" => {
                        let content_index = content_index.expect("thinking slot has an index");
                        let summary_text = join_content_text(item.get("summary"));
                        let content_text = join_content_text(item.get("content"));
                        if let AssistantContent::Thinking(block) = &mut output.content[content_index] {
                            let existing = block.thinking.clone();
                            block.thinking = if !summary_text.is_empty() {
                                summary_text
                            } else if !content_text.is_empty() {
                                content_text
                            } else {
                                existing
                            };
                            block.thinking_signature = Some(item.to_string());
                        }
                        if let Some(id) = item.get("id").and_then(Value::as_str) {
                            reasoning_blocks_by_id.insert(id.to_owned(), content_index);
                        }
                        let content = match &output.content[content_index] {
                            AssistantContent::Thinking(block) => block.thinking.clone(),
                            _ => String::new(),
                        };
                        sender.push(AssistantMessageEvent::ThinkingEnd {
                            content_index,
                            content,
                            partial: output.clone(),
                        });
                        output_slots.remove(&output_index);
                    }
                    Some("message") if slot_kind == "text" => {
                        let content_index = content_index.expect("text slot has an index");
                        let provider_text = join_message_content(item.get("content"));
                        let completed_text = if saw_hosted_web_search_call {
                            normalize_citation_text(&provider_text)
                        } else {
                            provider_text
                        };
                        let existing = match &output.content[content_index] {
                            AssistantContent::Text(block) => block.text.clone(),
                            _ => String::new(),
                        };
                        if let Some(remainder) = completed_text.strip_prefix(existing.as_str())
                            && !remainder.is_empty()
                        {
                            emit_text_delta(sender, content_index, remainder, output);
                        }
                        if let AssistantContent::Text(block) = &mut output.content[content_index] {
                            block.text = completed_text.clone();
                            block.text_signature = Some(encode_text_signature_v1(
                                item.get("id").and_then(Value::as_str).unwrap_or(""),
                                item.get("phase").and_then(Value::as_str),
                            ));
                        }
                        sender.push(AssistantMessageEvent::TextEnd {
                            content_index,
                            content: completed_text,
                            partial: output.clone(),
                        });
                        output_slots.remove(&output_index);
                    }
                    Some("function_call") if slot_kind == "toolCall" => {
                        let content_index = content_index.expect("tool slot has an index");
                        let Some(Slot::ToolCall { partial_json, .. }) = output_slots.get(&output_index) else {
                            continue;
                        };
                        if partial_json.is_none() {
                            continue;
                        }
                        let arguments = item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .filter(|arguments| !arguments.is_empty())
                            .map(str::to_owned)
                            .or_else(|| partial_json.clone())
                            .filter(|arguments| !arguments.is_empty())
                            .unwrap_or_else(|| "{}".to_owned());
                        let parsed = parse_arguments(Some(&arguments));
                        let namespace = item.get("namespace").and_then(Value::as_str).map(str::to_owned);
                        let tool_call = match &mut output.content[content_index] {
                            AssistantContent::ToolCall(block) => {
                                block.arguments = parsed;
                                if namespace.is_some() {
                                    block.namespace = namespace;
                                }
                                block.clone()
                            }
                            _ => continue,
                        };
                        if let Some(Slot::ToolCall { partial_json, .. }) = output_slots.get_mut(&output_index) {
                            *partial_json = None;
                        }
                        sender.push(AssistantMessageEvent::ToolCallEnd {
                            content_index,
                            tool_call,
                            partial: output.clone(),
                        });
                        output_slots.remove(&output_index);
                    }
                    Some("custom_tool_call") if slot_kind == "toolCall" => {
                        let content_index = content_index.expect("tool slot has an index");
                        let Some(Slot::ToolCall { custom_input, .. }) = output_slots.get(&output_index) else {
                            continue;
                        };
                        if custom_input.is_none() {
                            continue;
                        }
                        let input = item.get("input").and_then(Value::as_str);
                        let namespace = item.get("namespace").and_then(Value::as_str).map(str::to_owned);
                        let mut next_custom_input = output_slots.get(&output_index).and_then(|slot| match slot {
                            Slot::ToolCall { custom_input, .. } => custom_input.clone(),
                            _ => None,
                        });
                        let AssistantContent::ToolCall(block) = &mut output.content[content_index] else {
                            continue;
                        };
                        let next_input = input
                            .map(str::to_owned)
                            .unwrap_or_else(|| custom_tool_call_input(block, &next_custom_input));
                        let appended = append_custom_tool_call_input(block, &mut next_custom_input, &next_input, true);
                        let delta = match appended {
                            Ok(delta) => delta,
                            Err(error) => return Err(ResponsesStreamFailure::Message(error)),
                        };
                        if let Some(namespace) = namespace {
                            block.namespace = Some(namespace);
                        }
                        let tool_call = block.clone();
                        push_tool_call_delta(sender, content_index, delta, output);
                        if let Some(Slot::ToolCall { custom_input, .. }) = output_slots.get_mut(&output_index) {
                            *custom_input = None;
                        }
                        sender.push(AssistantMessageEvent::ToolCallEnd {
                            content_index,
                            tool_call,
                            partial: output.clone(),
                        });
                        output_slots.remove(&output_index);
                    }
                    _ => {}
                }
            }
            "response.completed" => {
                saw_terminal_response_event = true;
                let response = event.get("response").cloned().unwrap_or(Value::Null);
                finalize_response(&response, output, model, options, &reasoning_blocks_by_id)
                    .map_err(ResponsesStreamFailure::Message)?;
            }
            "response.incomplete" => {
                let response = event.get("response").cloned().unwrap_or(Value::Null);
                let reason = response
                    .get("incomplete_details")
                    .and_then(|details| details.get("reason"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if reason == "max_output_tokens" && output.content.is_empty() {
                    return Err(ResponsesStreamFailure::Message(format!(
                        "Incomplete response returned, reason: {reason}"
                    )));
                }
                saw_terminal_response_event = true;
                finalize_response(&response, output, model, options, &reasoning_blocks_by_id)
                    .map_err(ResponsesStreamFailure::Message)?;
            }
            "error" => {
                return Err(ResponsesStreamFailure::Message(format!(
                    "Error Code {}: {}",
                    js_string(event.get("code")),
                    js_string(event.get("message"))
                )));
            }
            "response.failed" => {
                let response = event.get("response").cloned().unwrap_or(Value::Null);
                output.raw_stop_reason = response.get("status").and_then(Value::as_str).map(str::to_owned);
                let error = response.get("error");
                let details = response.get("incomplete_details");
                let message = match error.filter(|error| is_truthy(error)) {
                    Some(error) => format!(
                        "{}: {}",
                        js_string(error.get("code").filter(|code| is_truthy(code))),
                        js_string(error.get("message").filter(|message| is_truthy(message)))
                    ),
                    None => match details
                        .and_then(|details| details.get("reason"))
                        .and_then(Value::as_str)
                    {
                        Some(reason) => format!("incomplete: {reason}"),
                        None => "Unknown error (no error details in response)".to_owned(),
                    },
                };
                return Err(ResponsesStreamFailure::Message(message));
            }
            _ => {}
        }
    }

    if !saw_terminal_response_event {
        return Err(ResponsesStreamFailure::Message(
            "OpenAI Responses stream ended before a terminal response event".to_owned(),
        ));
    }
    Ok(())
}

fn join_content_text(content: Option<&Value>) -> String {
    content
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.get("text").and_then(Value::as_str).unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .unwrap_or_default()
}

fn join_message_content(content: Option<&Value>) -> String {
    content
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    if item.get("type").and_then(Value::as_str) == Some("output_text") {
                        item.get("text").and_then(Value::as_str).unwrap_or("")
                    } else {
                        item.get("refusal").and_then(Value::as_str).unwrap_or("")
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Push the terminal `done` event for a processed message.
pub fn push_responses_done(sender: &AssistantMessageEventSender, output: &AssistantMessage) -> Result<(), String> {
    let Some(reason) = done_reason(output.stop_reason) else {
        return Err("OpenAI Responses stream ended without a stop reason".to_owned());
    };
    sender.push(AssistantMessageEvent::Done {
        reason,
        message: output.clone(),
    });
    Ok(())
}

/// Suppress unused warnings for the cancellation helper re-exported to callers.
pub fn cancelled(signal: Option<&CancellationToken>) -> bool {
    signal.is_some_and(CancellationToken::is_cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hosted_search_citations() {
        let text = "Alpha\u{e200}cite\u{e202}turn1a1\u{e201} beta.\u{e200}cite\u{e202}turn2b2\u{e201}Gamma".to_owned();
        assert_eq!(normalize_citation_text(&text), "Alpha beta. Gamma");
    }

    #[test]
    fn keeps_non_citation_annotations() {
        let text = "A\u{e200}other\u{e201}B".to_owned();
        assert_eq!(normalize_citation_text(&text), "A\u{e200}other\u{e201}B");
    }

    #[test]
    fn normalizes_citations_split_across_deltas() {
        let mut state = create_citation_text_state(None);
        let mut output = String::new();
        for delta in ["Alpha", "\u{e200}cite", "\u{e202}turn1a1", "\u{e201}", " beta"] {
            let normalized = normalize_citation_text_delta(&state, delta);
            state = normalized.state;
            output.push_str(&normalized.text);
        }
        output.push_str(&finish_citation_text(&state).text);
        assert_eq!(output, "Alpha beta");
    }

    #[test]
    fn id_normalization_matches_openai_shape() {
        assert_eq!(normalize_id_part("a/b+c=="), "a_b_c");
        assert_eq!(normalize_id_part(&"x".repeat(70)), "x".repeat(64));
    }
}
