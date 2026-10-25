//! Port of `packages/ai/src/api/openai-responses-shared.ts`.
//!
//! Message/tool conversion and the Responses SSE state machine. Request bodies
//! are built as JSON values to preserve the exact TypeScript wire shape.

use crate::api::openai_sdk::{ingress_metadata_text, validate_metadata_fields};
use pi_js_compat::js_json::JsString;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsObject, JsValue};
use serde_json::Value;

use crate::api::transform_messages::transform_messages_with;
use crate::models::calculate_cost;
use crate::request::BoxFuture;
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CompatFields, Context, DoneReason, ImageContent,
    Message, Model, ProviderNativeFormat, StopReason, TextContent, TextSignatureV1, TextSignatureVersion,
    ThinkingContent, Tool, ToolCall, Usage, UsageCost,
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
    pub resolve_service_tier: Option<ResolveServiceTier>,
    pub apply_service_tier_pricing: Option<ApplyServiceTierPricing>,
}

/// `ConvertResponsesMessagesOptions`.
#[derive(Clone, Default)]
pub struct ConvertResponsesMessagesOptions {
    pub include_system_prompt: Option<bool>,
}

/// `ConvertResponsesToolsOptions`.
#[derive(Clone, Default)]
pub struct ConvertResponsesToolsOptions {
    /// Outer `None` = absent, `Some(None)` = explicit `null`.
    pub strict: Option<Option<bool>>,
}

/// Failure while consuming stream events: either the event source failed or
/// the state machine rejected an event.
#[derive(Debug)]
pub enum ResponsesStreamFailure<E> {
    Source(E),
    Message(JsString),
}

/// Async event source for [`process_responses_stream`].
pub trait ResponsesEventSource {
    type Error;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<JsValue>, Self::Error>>;
}

/// In-memory event source used by tests and replay harnesses.
pub struct VecResponsesEventSource {
    events: std::vec::IntoIter<JsValue>,
}

impl VecResponsesEventSource {
    pub fn new<T: Into<JsValue>>(events: Vec<T>) -> Self {
        Self {
            events: events.into_iter().map(Into::into).collect::<Vec<_>>().into_iter(),
        }
    }
}

impl ResponsesEventSource for VecResponsesEventSource {
    type Error = std::convert::Infallible;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<JsValue>, Self::Error>> {
        Box::pin(async move { Ok(self.events.next()) })
    }
}

// =============================================================================
// Utilities
// =============================================================================

fn object(entries: Vec<(&str, JsValue)>) -> JsValue {
    let mut map = JsObject::new();
    for (key, value) in entries {
        map.insert(key.into(), value);
    }
    JsValue::Object(map)
}

fn js_string(value: Option<&JsValue>) -> String {
    match value {
        Some(JsValue::String(text)) => text.to_utf8().expect("scalar provider metadata is not UTF-8"),
        Some(JsValue::Null) => "null".to_owned(),
        Some(other) => pi_js_compat::js_json::stringify_lossless(other),
        None => "undefined".to_owned(),
    }
}

fn human_error_text(value: Option<&JsValue>) -> JsString {
    match value {
        Some(JsValue::String(text)) => text.clone(),
        _ => js_string(value).into(),
    }
}

fn is_truthy(value: &JsValue) -> bool {
    match value {
        JsValue::Null => false,
        JsValue::Bool(value) => *value,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(text) => !text.is_empty(),
        JsValue::Array(_) | JsValue::Object(_) => true,
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

fn convert_tool_result_output(model: &Model, content: &[crate::types::TextOrImageContent]) -> JsValue {
    let text_result = content
        .iter()
        .filter_map(|block| match block {
            crate::types::TextOrImageContent::Text(text) => Some(sanitize_surrogates(&text.text)),
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
        } else {
            "(see attached image)".into()
        };
        return JsValue::String(sanitize_surrogates(&text).into());
    }

    let mut output = Vec::new();
    if has_text {
        output.push(object(vec![
            ("type", JsValue::String("input_text".into())),
            ("text", JsValue::String(sanitize_surrogates(&text_result).into())),
        ]));
    }
    for image in images {
        output.push(object(vec![
            ("type", JsValue::String("input_image".into())),
            ("detail", JsValue::String("auto".into())),
            (
                "image_url",
                JsValue::String(format!("data:{};base64,{}", image.mime_type, image.data).into()),
            ),
        ]));
    }
    JsValue::Array(output)
}

// =============================================================================
// Hosted-search citation normalization
// =============================================================================

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CitationTextState {
    marker_buffer: pi_js_compat::js_json::JsString,
    pending_whitespace: pi_js_compat::js_json::JsString,
    removed_citation: bool,
    last_character: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedCitationText {
    state: CitationTextState,
    text: pi_js_compat::js_json::JsString,
}

fn create_citation_text_state(last_character: Option<u16>) -> CitationTextState {
    CitationTextState {
        last_character,
        ..CitationTextState::default()
    }
}

fn is_raw_hosted_search_citation(marker: &pi_js_compat::js_json::JsString) -> bool {
    if !marker.starts_with(RAW_CITATION_PREFIX) || !marker.ends_with(RAW_ANNOTATION_END) {
        return false;
    }
    let payload = marker.slice(RAW_CITATION_PREFIX.encode_utf16().count(), marker.len() - 1);
    let references = payload.split(RAW_ANNOTATION_SEPARATOR).collect::<Vec<_>>();
    !references.is_empty() && references.iter().all(is_citation_reference)
}

fn is_citation_reference(reference: &pi_js_compat::js_json::JsString) -> bool {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| regex::Regex::new(r"^turn[0-9]+[A-Za-z][A-Za-z0-9_-]*[0-9]+$").expect("citation regex"))
        .is_match(&match reference.to_utf8() {
            Ok(text) => text,
            Err(_) => return false,
        })
}

fn get_citation_boundary_separator(last_character: Option<u16>, next_character: u16) -> &'static str {
    let Some(last_character) = last_character else {
        return "";
    };
    if pi_js_compat::string::is_js_whitespace_unit(last_character)
        || pi_js_compat::string::is_js_whitespace_unit(next_character)
    {
        return "";
    }
    if NO_SPACE_AFTER
        .iter()
        .any(|c| u32::from(*c) == u32::from(last_character))
        || NO_SPACE_BEFORE
            .iter()
            .any(|c| u32::from(*c) == u32::from(next_character))
    {
        return "";
    }
    " "
}

fn append_visible_citation_text(
    state: &CitationTextState,
    text: &pi_js_compat::js_json::JsString,
) -> NormalizedCitationText {
    let mut pending_whitespace = state.pending_whitespace.clone();
    let mut removed_citation = state.removed_citation;
    let mut last_character = state.last_character;
    let mut normalized_text = pi_js_compat::js_json::JsString::default();

    for character in text.units().iter().copied() {
        if character == u16::from(b' ') || character == u16::from(b'\t') {
            pending_whitespace.push_unit(character);
            continue;
        }
        if removed_citation {
            normalized_text.push_str(get_citation_boundary_separator(last_character, character));
        } else {
            normalized_text.push_str(&pending_whitespace);
        }
        normalized_text.push_unit(character);
        pending_whitespace = Default::default();
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

fn find_trailing_citation_buffer_start(text: &pi_js_compat::js_json::JsString) -> Option<usize> {
    let marker_start_index = text
        .units()
        .iter()
        .rposition(|unit| u32::from(*unit) == u32::from(RAW_ANNOTATION_START))?;
    let marker_candidate = text.slice(marker_start_index, text.len());
    if marker_candidate.contains(RAW_ANNOTATION_END) {
        return None;
    }
    let could_be_citation = pi_js_compat::js_json::JsString::from(RAW_CITATION_PREFIX).starts_with(&marker_candidate)
        || marker_candidate.starts_with(RAW_CITATION_PREFIX);
    could_be_citation.then_some(marker_start_index)
}

fn normalize_closed_citation_markers(
    state: &CitationTextState,
    text: &pi_js_compat::js_json::JsString,
) -> NormalizedCitationText {
    let mut current_state = state.clone();
    let mut normalized_text = pi_js_compat::js_json::JsString::default();
    let mut text_offset = 0;

    while let Some(relative_start) = text.slice(text_offset, text.len()).find(RAW_ANNOTATION_START) {
        let marker_start_index = text_offset + relative_start;
        let search_start = marker_start_index + 1;
        let Some(relative_end) = text.slice(search_start, text.len()).find(RAW_ANNOTATION_END) else {
            break;
        };
        let marker_end = search_start + relative_end + 1;

        let visible = append_visible_citation_text(&current_state, &text.slice(text_offset, marker_start_index));
        current_state = visible.state;
        normalized_text.push_str(&visible.text);

        let marker = text.slice(marker_start_index, marker_end);
        if is_raw_hosted_search_citation(&marker) {
            current_state.removed_citation = true;
        } else {
            let preserved = append_visible_citation_text(&current_state, &marker);
            current_state = preserved.state;
            normalized_text.push_str(&preserved.text);
        }
        text_offset = marker_end;
    }

    let trailing = append_visible_citation_text(&current_state, &text.slice(text_offset, text.len()));
    NormalizedCitationText {
        state: trailing.state,
        text: normalized_text + &trailing.text,
    }
}

fn normalize_citation_text_delta(
    state: &CitationTextState,
    delta: &pi_js_compat::js_json::JsString,
) -> NormalizedCitationText {
    let combined_text = state.marker_buffer.clone() + delta;
    let marker_buffer_start = find_trailing_citation_buffer_start(&combined_text);
    let (stable_text, marker_buffer) = match marker_buffer_start {
        Some(index) => (
            combined_text.slice(0, index),
            combined_text.slice(index, combined_text.len()),
        ),
        None => (combined_text, pi_js_compat::js_json::JsString::default()),
    };
    let mut without_buffer = state.clone();
    without_buffer.marker_buffer = Default::default();
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
    current_state.marker_buffer = Default::default();
    let mut normalized_text = pi_js_compat::js_json::JsString::default();

    if !state.marker_buffer.is_empty() {
        let preserved = append_visible_citation_text(&current_state, &state.marker_buffer);
        current_state = preserved.state;
        normalized_text = preserved.text;
    }

    if current_state.removed_citation {
        current_state.pending_whitespace = Default::default();
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

fn normalize_citation_text(text: &pi_js_compat::js_json::JsString) -> pi_js_compat::js_json::JsString {
    let normalized = normalize_citation_text_delta(&create_citation_text_state(None), text);
    normalized.text + &finish_citation_text(&normalized.state).text
}

// =============================================================================
// Message conversion
// =============================================================================

fn normalize_id_part(part: &str) -> String {
    let sanitized: String = part
        .encode_utf16()
        .map(|unit| {
            char::from_u32(u32::from(unit))
                .filter(|character| character.is_ascii_alphanumeric() || *character == '_' || *character == '-')
                .unwrap_or('_')
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
    let mut parts = id.split('|');
    (parts.next().unwrap_or_default(), parts.next())
}

pub fn convert_responses_messages(
    model: &Model,
    context: &Context,
    allowed_tool_call_providers: &BTreeSet<String>,
    options: Option<&ConvertResponsesMessagesOptions>,
) -> Result<Vec<JsValue>, JsString> {
    let empty_options = ConvertResponsesMessagesOptions::default();
    let options = options.unwrap_or(&empty_options);
    let mut messages = Vec::new();

    let normalize_tool_call_id = |id: &str, _target: &Model, source: &AssistantMessage| -> String {
        if !allowed_tool_call_providers.contains(&model.provider) {
            return normalize_id_part(id);
        }
        let (call_id, Some(item_id)) = split_tool_call_id(id) else {
            return normalize_id_part(id);
        };
        let normalized_call_id = normalize_id_part(call_id);
        let is_foreign = source.provider != model.provider || source.api != model.api;
        let mut normalized_item_id = if is_foreign {
            build_foreign_responses_item_id(item_id)
        } else {
            normalize_id_part(item_id)
        };
        if !normalized_item_id.starts_with("fc_") {
            normalized_item_id = normalize_id_part(&format!("fc_{normalized_item_id}"));
        }
        format!("{normalized_call_id}|{normalized_item_id}")
    };

    let transformed_messages = transform_messages_with(
        &context.messages,
        model,
        Some(&normalize_tool_call_id),
        crate::request::now_millis() as f64,
    );

    let include_system_prompt = options.include_system_prompt.unwrap_or(true);
    if include_system_prompt
        && let Some(system_prompt) = context.system_prompt.as_ref().filter(|prompt| !prompt.is_empty())
    {
        // Frozen shared268: direct !==false, regardless of the model's API.
        let supports_developer_role = model
            .compat
            .as_ref()
            .and_then(|compat| compat.raw("supportsDeveloperRole"))
            != Some(&JsValue::Bool(false));
        let role = if model.reasoning && supports_developer_role {
            "developer"
        } else {
            "system"
        };
        messages.push(object(vec![
            ("role", JsValue::String(role.into())),
            ("content", JsValue::String(sanitize_surrogates(system_prompt).into())),
        ]));
    }

    let mut msg_index = 0;
    let mut emitted_tool_call_ids: BTreeSet<String> = BTreeSet::new();
    for msg in &transformed_messages {
        match msg {
            Message::User(user) => {
                if let Some(native) = &user.provider_native
                    && native.provider == model.provider
                    && native.api == model.api
                    && native.format == ProviderNativeFormat::OpenAIResponsesInput
                    && let JsValue::Array(items) = &native.value
                {
                    messages.extend(items.iter().cloned());
                    continue;
                }

                match &user.content {
                    crate::types::UserContent::Text(text) => {
                        messages.push(object(vec![
                            ("role", JsValue::String("user".into())),
                            (
                                "content",
                                JsValue::Array(vec![object(vec![
                                    ("type", JsValue::String("input_text".into())),
                                    ("text", JsValue::String(sanitize_surrogates(text).into())),
                                ])]),
                            ),
                        ]));
                    }
                    crate::types::UserContent::Blocks(blocks) => {
                        let content = blocks
                            .iter()
                            .map(|block| match block {
                                crate::types::TextOrImageContent::Text(text) => object(vec![
                                    ("type", JsValue::String("input_text".into())),
                                    ("text", JsValue::String(sanitize_surrogates(&text.text).into())),
                                ]),
                                crate::types::TextOrImageContent::Image(image) => object(vec![
                                    ("type", JsValue::String("input_image".into())),
                                    ("detail", JsValue::String("auto".into())),
                                    (
                                        "image_url",
                                        JsValue::String(
                                            format!("data:{};base64,{}", image.mime_type, image.data).into(),
                                        ),
                                    ),
                                ]),
                            })
                            .collect::<Vec<_>>();
                        if content.is_empty() {
                            continue;
                        }
                        messages.push(object(vec![
                            ("role", JsValue::String("user".into())),
                            ("content", JsValue::Array(content)),
                        ]));
                    }
                }
            }
            Message::Assistant(assistant) => {
                let mut output = Vec::new();
                let is_same_provider_and_api = assistant.provider == model.provider && assistant.api == model.api;
                let is_different_model = is_same_provider_and_api && assistant.model != model.id;
                let mut text_block_index = 0;

                for block in &assistant.content {
                    match block {
                        AssistantContent::Thinking(thinking) => {
                            if let Some(signature) = thinking
                                .thinking_signature
                                .as_deref()
                                .filter(|signature| !signature.is_empty())
                            {
                                let item = pi_js_compat::js_json::parse_lossless(signature)
                                    .map_err(|error| error.into_message())?;
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
                                Some(parsed) if parsed.id.encode_utf16().count() > 64 => {
                                    format!("msg_{}", short_hash(&parsed.id))
                                }
                                Some(parsed) => parsed.id.clone(),
                            };
                            let mut message = object(vec![
                                ("type", JsValue::String("message".into())),
                                ("role", JsValue::String("assistant".into())),
                                (
                                    "content",
                                    JsValue::Array(vec![object(vec![
                                        ("type", JsValue::String("output_text".into())),
                                        ("text", JsValue::String(sanitize_surrogates(&text.text).into())),
                                        ("annotations", JsValue::Array(Vec::new())),
                                    ])]),
                                ),
                                ("status", JsValue::String("completed".into())),
                                ("id", JsValue::String(msg_id.into())),
                            ]);
                            if let Some(phase) = parsed_signature.and_then(|parsed| parsed.phase)
                                && let JsValue::Object(map) = &mut message
                            {
                                map.insert("phase".into(), JsValue::String(phase.into()));
                            }
                            output.push(message);
                        }
                        AssistantContent::ToolCall(tool_call) => {
                            let (call_id, item_id_raw) = split_tool_call_id(&tool_call.id);
                            emitted_tool_call_ids.insert(call_id.into());
                            let mut item_id = item_id_raw.map(str::to_owned);
                            if is_different_model && item_id.as_deref().is_some_and(|id| id.starts_with("fc_")) {
                                item_id = None;
                            }
                            let entries = vec![
                                ("type", JsValue::String("function_call".into())),
                                (
                                    "id",
                                    item_id.map(|id| JsValue::String(id.into())).unwrap_or(JsValue::Null),
                                ),
                                ("call_id", JsValue::String(call_id.into())),
                                ("name", JsValue::String(tool_call.name.clone().into())),
                                (
                                    "arguments",
                                    JsValue::String(
                                        pi_js_compat::js_json::stringify_lossless(&JsValue::Object(
                                            tool_call.arguments.clone(),
                                        ))
                                        .into(),
                                    ),
                                ),
                            ];
                            let mut item = object(entries);
                            if let JsValue::Object(map) = &mut item
                                && map.get("id") == Some(&JsValue::Null)
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

                messages.push(object(vec![
                    ("type", JsValue::String("function_call_output".into())),
                    ("call_id", JsValue::String(call_id.into())),
                    ("output", output),
                ]));
            }
        }
        msg_index += 1;
    }

    Ok(messages)
}

// =============================================================================
// Tool conversion
// =============================================================================

pub fn convert_responses_tools(tools: &[Tool], options: Option<&ConvertResponsesToolsOptions>) -> JsValue {
    let empty_options = ConvertResponsesToolsOptions::default();
    let options = options.unwrap_or(&empty_options);
    let default_strict = match options.strict {
        None => Some(false),
        Some(strict) => strict,
    };
    let converted = tools
        .iter()
        .map(|tool| {
            object(vec![
                ("type", JsValue::String("function".into())),
                ("name", JsValue::String(tool.name.clone().into())),
                ("description", JsValue::String(tool.description.clone())),
                ("parameters", tool.parameters.clone().into()),
                ("strict", default_strict.map(JsValue::Bool).unwrap_or(JsValue::Null)),
            ])
        })
        .collect();
    JsValue::Array(converted)
}

// =============================================================================
// Stream processing
// =============================================================================

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

fn push_tool_call_delta(
    sender: &AssistantMessageEventSender,
    content_index: usize,
    delta: Option<impl Into<pi_js_compat::js_json::JsString>>,
    output: &AssistantMessage,
) {
    let Some(delta) = delta else {
        return;
    };
    sender.push(AssistantMessageEvent::ToolCallDelta {
        content_index,
        delta: delta.into(),
        partial: output.clone(),
    });
}

fn parse_arguments(partial_json: Option<&pi_js_compat::js_json::JsString>) -> pi_js_compat::js_json::JsObject {
    match parse_streaming_json(partial_json) {
        pi_js_compat::js_json::JsValue::Object(map) => map,
        _ => Default::default(),
    }
}

fn create_slot(
    slots: &mut BTreeMap<Option<u64>, Slot>,
    output_index: Option<u64>,
    item: &JsValue,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
) -> Option<()> {
    match item.get("type").and_then(ingress_metadata_text).as_deref() {
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
                item.get("name")
                    .and_then(ingress_metadata_text)
                    .as_deref()
                    .unwrap_or(""),
                Default::default(),
            );
            block.partial_json = Some(
                item.get("arguments")
                    .and_then(JsValue::as_string)
                    .filter(|arguments| !arguments.is_empty())
                    .cloned()
                    .unwrap_or_default(),
            );
            output.content.push(AssistantContent::ToolCall(block));
            let content_index = output.content.len() - 1;
            slots.insert(output_index, Slot::ToolCall { content_index });
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
    item: &JsValue,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
) -> Option<()> {
    if slots.contains_key(&output_index) {
        return Some(());
    }
    create_slot(slots, output_index, item, output, sender)
}

fn emit_text_delta(
    sender: &AssistantMessageEventSender,
    content_index: usize,
    delta: &pi_js_compat::js_json::JsString,
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
        delta: delta.into(),
        partial: output.clone(),
    });
}

fn append_provider_text_delta(
    sender: &AssistantMessageEventSender,
    content_index: usize,
    delta: &pi_js_compat::js_json::JsString,
    citation_state: &mut Option<CitationTextState>,
    output: &mut AssistantMessage,
) {
    let last_character = match &output.content[content_index] {
        AssistantContent::Text(block) => block.text.units().last().copied(),
        _ => None,
    };
    let state = citation_state
        .clone()
        .unwrap_or_else(|| create_citation_text_state(last_character));
    let normalized = normalize_citation_text_delta(&state, &delta.into());
    *citation_state = Some(normalized.state);
    emit_text_delta(sender, content_index, &normalized.text, output);
}

fn finalize_response(
    response: &JsValue,
    output: &mut AssistantMessage,
    model: &Model,
    options: Option<&OpenAIResponsesStreamOptions>,
) -> Result<(), String> {
    let empty = Vec::new();
    let response_output = response.get("output").and_then(JsValue::as_array).unwrap_or(&empty);

    if let Some(id) = response
        .get("id")
        .and_then(ingress_metadata_text)
        .as_deref()
        .filter(|id| !id.is_empty())
    {
        output.response_id = Some(id.to_owned());
    }

    let image_generation_item = response_output.iter().find(|item| {
        item.get("type").and_then(ingress_metadata_text).as_deref() == Some("image_generation_call")
            && item.get("status").and_then(ingress_metadata_text).as_deref() == Some("completed")
            && item.get("result").is_some_and(is_truthy)
    });
    if let Some(item) = image_generation_item
        && let Some(result) = item.get("result").and_then(ingress_metadata_text).as_deref()
    {
        output.image_generation_result = Some(ImageContent::new(result, "image/png"));
    }

    if let Some(usage) = response.get("usage").filter(|usage| is_truthy(usage)) {
        let input_details = usage.get("input_tokens_details");
        let cached_tokens = input_details
            .and_then(|details| details.get("cached_tokens"))
            .and_then(JsValue::as_f64)
            .unwrap_or(0.0);
        let input_tokens = usage.get("input_tokens").and_then(JsValue::as_f64).unwrap_or(0.0);
        output.usage = Usage {
            input: input_tokens - cached_tokens,
            output: usage.get("output_tokens").and_then(JsValue::as_f64).unwrap_or(0.0),
            cache_read: cached_tokens,
            cache_write: 0.0,
            cache_write_1h: None,
            reasoning: usage
                .get("output_tokens_details")
                .and_then(|details| details.get("reasoning_tokens"))
                .and_then(JsValue::as_f64)
                .unwrap_or(0.0)
                .into(),
            total_tokens: usage.get("total_tokens").and_then(JsValue::as_f64).unwrap_or(0.0),
            cost: UsageCost::default(),
        };
    }
    output.usage.cost = calculate_cost(model, &output.usage);

    let response_service_tier_text = response.get("service_tier").and_then(ingress_metadata_text);
    let response_service_tier = response_service_tier_text.as_deref();
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

    let status_text = response.get("status").and_then(ingress_metadata_text);
    let status = status_text.as_deref();
    let mapped = map_stop_reason(status)?;
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
    error_message: Option<JsString>,
}

fn map_stop_reason(status: Option<&str>) -> Result<MappedStopReason, String> {
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
        "incomplete" => MappedStopReason {
            stop_reason: StopReason::Length,
            error_message: None,
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
        StopReason::Error | StopReason::Aborted => None,
    }
}

fn validate_response_item_metadata(item: &JsValue) -> Result<(), String> {
    validate_metadata_fields(item, &["type"])?;
    match ingress_metadata_text(&item["type"]).as_deref() {
        Some("message") => {
            validate_metadata_fields(item, &["id", "phase"])?;
            if let Some(content) = item.get("content").and_then(JsValue::as_array) {
                for part in content {
                    validate_metadata_fields(part, &["type"])?;
                }
            }
        }
        Some("reasoning") => validate_metadata_fields(item, &["id"])?,
        Some("function_call" | "custom_tool_call") => {
            validate_metadata_fields(item, &["id", "call_id", "name", "namespace"])?
        }
        Some("image_generation_call") => validate_metadata_fields(item, &["status", "result"])?,
        _ => {}
    }
    Ok(())
}

pub(crate) fn validate_responses_metadata(event: &JsValue) -> Result<(), String> {
    validate_metadata_fields(event, &["type"])?;
    match ingress_metadata_text(&event["type"]).as_deref() {
        Some("error") => {
            validate_metadata_fields(event, &["code"])?;
            if let Some(error) = event.get("error") {
                validate_metadata_fields(error, &["code"])?;
            }
        }
        Some("response.image_generation_call.partial_image") => {
            validate_metadata_fields(event, &["partial_image_b64"])?
        }
        _ => {}
    }
    if let Some(item) = event.get("item") {
        validate_response_item_metadata(item)?;
    }
    if let Some(response) = event.get("response") {
        validate_response_metadata(response)?;
    }
    Ok(())
}

fn validate_response_metadata(response: &JsValue) -> Result<(), String> {
    validate_metadata_fields(response, &["id", "status", "service_tier"])?;
    if let Some(details) = response.get("incomplete_details") {
        validate_metadata_fields(details, &["reason"])?;
    }
    if let Some(error) = response.get("error") {
        validate_metadata_fields(error, &["code"])?;
    }
    if let Some(items) = response.get("output").and_then(JsValue::as_array) {
        for item in items {
            validate_response_item_metadata(item)?;
        }
    }
    Ok(())
}

pub async fn process_responses_stream<S: ResponsesEventSource>(
    source: &mut S,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    model: &Model,
    options: Option<&OpenAIResponsesStreamOptions>,
) -> Result<(), ResponsesStreamFailure<S::Error>> {
    let mut saw_terminal_response_event = false;
    let mut output_slots: BTreeMap<Option<u64>, Slot> = BTreeMap::new();

    loop {
        let event = match source.next_event().await {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(error) => return Err(ResponsesStreamFailure::Source(error)),
        };
        validate_responses_metadata(&event).map_err(|error| ResponsesStreamFailure::Message(error.into()))?;
        let event_type = event.get("type").and_then(ingress_metadata_text).unwrap_or_default();
        let output_index = event.get("output_index").and_then(JsValue::as_u64);

        match event_type.as_str() {
            "response.created" => {
                if let Some(id) = event
                    .get("response")
                    .and_then(|response| response.get("id"))
                    .and_then(ingress_metadata_text)
                    .as_deref()
                {
                    output.response_id = Some(id.to_owned());
                }
            }
            "response.output_item.added" => {
                let item = event.get("item").cloned().unwrap_or(JsValue::Null);
                create_slot(&mut output_slots, output_index, &item, output, sender);
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "thinking") else {
                    continue;
                };
                let delta = event
                    .get("delta")
                    .and_then(JsValue::as_string)
                    .cloned()
                    .unwrap_or_default();
                if let AssistantContent::Thinking(block) = &mut output.content[content_index] {
                    block.thinking.push_str(&delta);
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
                    delta: "\n\n".to_owned().into(),
                    partial: output.clone(),
                });
            }
            "response.output_text.delta" | "response.refusal.delta" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "text") else {
                    continue;
                };
                let delta = event
                    .get("delta")
                    .and_then(JsValue::as_string)
                    .cloned()
                    .unwrap_or_default();
                let mut citation_state = match output_slots.get(&output_index) {
                    Some(Slot::Text { citation_state, .. }) => citation_state.clone(),
                    _ => None,
                };
                append_provider_text_delta(sender, content_index, &delta, &mut citation_state, output);
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
                let delta = event
                    .get("delta")
                    .and_then(JsValue::as_string)
                    .cloned()
                    .unwrap_or_default();
                let AssistantContent::ToolCall(block) = &mut output.content[content_index] else {
                    continue;
                };
                let Some(partial_json) = block.partial_json.as_mut() else {
                    continue;
                };
                partial_json.push_str(&delta);
                block.arguments = parse_arguments(Some(partial_json));
                push_tool_call_delta(sender, content_index, Some(delta.to_owned()), output);
            }
            "response.function_call_arguments.done" => {
                let Some(content_index) = get_slot(&output_slots, output_index, "toolCall") else {
                    continue;
                };
                let arguments = event
                    .get("arguments")
                    .and_then(JsValue::as_string)
                    .cloned()
                    .unwrap_or_default();
                let AssistantContent::ToolCall(block) = &mut output.content[content_index] else {
                    continue;
                };
                let Some(previous) = block.partial_json.as_ref() else {
                    continue;
                };
                let delta = arguments.strip_prefix(previous).filter(|delta| !delta.is_empty());
                block.partial_json = Some(arguments.clone());
                block.arguments = parse_arguments(Some(&arguments));
                push_tool_call_delta(sender, content_index, delta, output);
            }
            "response.image_generation_call.partial_image" => {
                output.image_generation_result = Some(ImageContent::new(
                    event
                        .get("partial_image_b64")
                        .and_then(ingress_metadata_text)
                        .as_deref()
                        .unwrap_or(""),
                    "image/png",
                ));
            }
            "response.output_item.done" => {
                let item = event.get("item").cloned().unwrap_or(JsValue::Null);
                get_or_create_slot(&mut output_slots, output_index, &item, output, sender);
                let slot_kind = match output_slots.get(&output_index) {
                    Some(Slot::Thinking { .. }) => "thinking",
                    Some(Slot::Text { .. }) => "text",
                    Some(Slot::ToolCall { .. }) => "toolCall",
                    None => "",
                };
                let content_index = get_slot(&output_slots, output_index, slot_kind);

                match item.get("type").and_then(ingress_metadata_text).as_deref() {
                    Some("image_generation_call")
                        if item.get("status").and_then(ingress_metadata_text).as_deref() == Some("completed")
                            && item.get("result").is_some_and(is_truthy) =>
                    {
                        output.image_generation_result = Some(ImageContent::new(
                            item.get("result")
                                .and_then(ingress_metadata_text)
                                .as_deref()
                                .unwrap_or(""),
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
                            block.thinking_signature = Some(pi_js_compat::js_json::stringify_lossless(&item));
                        }
                        let content = match &output.content[content_index] {
                            AssistantContent::Thinking(block) => block.thinking.clone(),
                            _ => String::new().into(),
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
                        let completed_text = normalize_citation_text(&provider_text);
                        let existing = match &output.content[content_index] {
                            AssistantContent::Text(block) => block.text.clone(),
                            _ => Default::default(),
                        };
                        if let Some(remainder) = completed_text.strip_prefix(&existing)
                            && !remainder.is_empty()
                        {
                            emit_text_delta(sender, content_index, &remainder, output);
                        }
                        if let AssistantContent::Text(block) = &mut output.content[content_index] {
                            block.text = completed_text.clone();
                            block.text_signature = Some(encode_text_signature_v1(
                                item.get("id").and_then(ingress_metadata_text).as_deref().unwrap_or(""),
                                item.get("phase").and_then(ingress_metadata_text).as_deref(),
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
                        let AssistantContent::ToolCall(block) = &mut output.content[content_index] else {
                            continue;
                        };
                        let arguments = item
                            .get("arguments")
                            .and_then(JsValue::as_string)
                            .filter(|arguments| !arguments.is_empty())
                            .map(pi_js_compat::js_json::JsString::from)
                            .or_else(|| block.partial_json.clone())
                            .filter(|arguments| !arguments.is_empty())
                            .unwrap_or_else(|| "{}".into());
                        block.arguments = parse_arguments(Some(&arguments));
                        block.partial_json = None;
                        let tool_call = block.clone();
                        sender.push(AssistantMessageEvent::ToolCallEnd {
                            content_index,
                            tool_call: Box::new(tool_call),
                            partial: output.clone(),
                        });
                        output_slots.remove(&output_index);
                    }
                    _ => {}
                }
            }
            "response.completed" => {
                saw_terminal_response_event = true;
                let response = event.get("response").cloned().unwrap_or(JsValue::Null);
                finalize_response(&response, output, model, options)
                    .map_err(|error| ResponsesStreamFailure::Message(error.into()))?;
            }
            "response.incomplete" => {
                let response = event.get("response").cloned().unwrap_or(JsValue::Null);
                let reason = response
                    .get("incomplete_details")
                    .and_then(|details| details.get("reason"))
                    .and_then(ingress_metadata_text)
                    .unwrap_or_else(|| "unknown".to_owned());
                if reason != "max_output_tokens" {
                    return Err(ResponsesStreamFailure::Message(
                        format!("Incomplete response returned, reason: {reason}").into(),
                    ));
                }
                saw_terminal_response_event = true;
                finalize_response(&response, output, model, options)
                    .map_err(|error| ResponsesStreamFailure::Message(error.into()))?;
            }
            "error" => {
                return Err(ResponsesStreamFailure::Message(JsString::join(
                    [
                        format!("Error Code {}: ", js_string(event.get("code"))).into(),
                        human_error_text(event.get("message")),
                    ],
                    "",
                )));
            }
            "response.failed" => {
                let response = event.get("response").cloned().unwrap_or(JsValue::Null);
                let error = response.get("error");
                let details = response.get("incomplete_details");
                let message = match error.filter(|error| is_truthy(error)) {
                    Some(error) => {
                        let code = error
                            .get("code")
                            .filter(|code| is_truthy(code))
                            .map(|code| js_string(Some(code)))
                            .unwrap_or_else(|| "unknown".into());
                        let message = error
                            .get("message")
                            .filter(|message| is_truthy(message))
                            .map(|message| human_error_text(Some(message)))
                            .unwrap_or_else(|| "no message".into());
                        pi_js_compat::js_format!("{}: {}", code, message)
                    }
                    None => match details
                        .and_then(|details| details.get("reason"))
                        .and_then(ingress_metadata_text)
                        .as_deref()
                    {
                        Some(reason) => format!("incomplete: {reason}"),
                        None => "Unknown error (no error details in response)".to_owned(),
                    }
                    .into(),
                };
                return Err(ResponsesStreamFailure::Message(message));
            }
            _ => {}
        }
    }

    if !saw_terminal_response_event {
        return Err(ResponsesStreamFailure::Message(
            "OpenAI Responses stream ended before a terminal response event"
                .to_owned()
                .into(),
        ));
    }
    Ok(())
}

fn join_content_text(content: Option<&JsValue>) -> pi_js_compat::js_json::JsString {
    content
        .and_then(JsValue::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.get("text")
                        .and_then(JsValue::as_string)
                        .cloned()
                        .unwrap_or_default()
                })
                .enumerate()
                .fold(pi_js_compat::js_json::JsString::default(), |mut text, (index, part)| {
                    if index > 0 {
                        text.push_str("\n\n");
                    }
                    text.push_str(part);
                    text
                })
        })
        .unwrap_or_default()
}

fn join_message_content(content: Option<&JsValue>) -> pi_js_compat::js_json::JsString {
    content
        .and_then(JsValue::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    if item.get("type").and_then(ingress_metadata_text).as_deref() == Some("output_text") {
                        item.get("text")
                            .and_then(JsValue::as_string)
                            .cloned()
                            .unwrap_or_default()
                    } else {
                        item.get("refusal")
                            .and_then(JsValue::as_string)
                            .cloned()
                            .unwrap_or_default()
                    }
                })
                .fold(pi_js_compat::js_json::JsString::default(), |mut text, part| {
                    text.push_str(part);
                    text
                })
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
pub fn cancelled(signal: Option<&AbortSignal>) -> bool {
    signal.is_some_and(AbortSignal::aborted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_hosted_search_citations() {
        let text = "Alpha\u{e200}cite\u{e202}turn1a1\u{e201} beta.\u{e200}cite\u{e202}turn2b2\u{e201}Gamma".to_owned();
        assert_eq!(normalize_citation_text(&text.into()), "Alpha beta. Gamma");
    }

    #[test]
    fn keeps_non_citation_annotations() {
        let text = "A\u{e200}other\u{e201}B".to_owned();
        assert_eq!(normalize_citation_text(&text.into()), "A\u{e200}other\u{e201}B");
    }

    #[test]
    fn normalizes_citations_split_across_deltas() {
        let mut state = create_citation_text_state(None);
        let mut output = JsString::default();
        for delta in ["Alpha", "\u{e200}cite", "\u{e202}turn1a1", "\u{e201}", " beta"] {
            let normalized = normalize_citation_text_delta(&state, &delta.into());
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
