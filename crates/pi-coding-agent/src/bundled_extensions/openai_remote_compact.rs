//! Port of `packages/coding-agent/extensions/openai-remote-compact/src/index.ts`.
//!
//! Registers a compaction handler that replaces local summarization with the
//! provider-native OpenAI compaction endpoint.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use pi_agent_core::harness::messages::convert_to_llm;
use pi_ai::request::{BoxFuture, HttpRequest};
use pi_ai::types::{
    AssistantContent, Message, Modality, Model, ProviderHeaders, ProviderNativeContext, ProviderNativeFormat,
    TextOrImageContent, UserContent,
};
use pi_js_compat::base64::buffer_from_base64;
use pi_js_compat::collator::locale_compare;
use pi_js_compat::string::{js_trim, utf16_len};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::core::compaction::{CompactionResult, CompactionSourceInfo, CompactionSourceType};
use crate::core::extensions::loader::ExtensionFactory;
use crate::core::extensions::types::{
    CompactionEvent, CompactionEventResult, ExtensionApi, ExtensionContext, HandlerError, NotifyType,
};
use crate::core::session_manager::SessionEntry;

pub const OPENAI_REMOTE_COMPACTION_SUMMARY: &str = "OpenAI native compaction stored in session entry details.";
const DETAILS_TYPE: &str = "openai-remote-compaction";
const DETAILS_VERSION: f64 = 1.0;
const MAX_OPENAI_REMOTE_COMPACT_CONTEXT_CHARS: usize = 400_000;
const CODEX_REMOTE_COMPACTION_MODEL: &str = "gpt-5.6-terra";
const OPENAI_REMOTE_COMPACTION_GUIDANCE: &str = "Create a compact context checkpoint for continuing the work, not a conversational reply.

- Deduplicate repeated or semantically equivalent content. Represent each fact, message, and tool result once.
- Collapse repeated rows, messages, and tool outputs into one canonical statement. Include a repetition count only when it affects diagnosis.
- Preserve the current goal, user-stated constraints, decisions and reasons, completed work, current status, unresolved blockers, next steps, and exact artifacts needed to continue.
- Prefer the latest state over superseded state. Record unresolved contradictions explicitly.
- Integrate relevant prior compacted context instead of copying it verbatim.
- Do not copy system, developer, or startup instruction packets into replacement history.
- Do not invent completion, evidence, file changes, commands, or test results.
- Prioritize the current task, then parked tasks, then only the earlier history needed to continue.";

/// Details stored in a compaction entry by the remote-compaction extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiRemoteCompactionDetails {
    #[serde(rename = "type")]
    pub details_type: String,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub version: f64,
    pub provider: String,
    pub api: String,
    pub model: String,
    pub endpoint: String,
    pub replacement_history: Vec<Value>,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub replacement_history_bytes: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub replacement_history_tokens: f64,
}

/// Request body sent to the provider-native compaction endpoint.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpenAiCompactPayload {
    pub model: String,
    pub input: Vec<Value>,
    pub instructions: String,
    pub tools: Vec<Value>,
    pub parallel_tool_calls: bool,
}

/// Registers the extension (`openAIRemoteCompactExtension`).
pub fn openai_remote_compact_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_compaction(|event: CompactionEvent, ctx: ExtensionContext| {
        Box::pin(async move { handle_compaction(event, ctx).await }) as BoxFuture<'static, _>
    })
    .map_err(HandlerError::new)
}

/// Inline extension factory for the first-party registration.
pub fn openai_remote_compact_extension_factory() -> ExtensionFactory {
    Arc::new(|pi| {
        let result = openai_remote_compact_extension(pi);
        Box::pin(async move { result })
    })
}

/// `handleCompaction`.
pub async fn handle_compaction(
    event: CompactionEvent,
    ctx: ExtensionContext,
) -> Result<Option<CompactionEventResult>, HandlerError> {
    let Some(model) = ctx.model()? else {
        return Ok(None);
    };
    if !is_openai_responses_model(Some(&model)) {
        return Ok(None);
    }
    let previous_replacement_history = find_latest_openai_replacement_history(&event.branch_entries, &model);
    let auth = ctx.model_registry()?.get_api_key_and_headers(&model).await;
    let (api_key, headers) = match auth {
        crate::core::model_registry::ResolvedRequestAuth::Ok { api_key, headers, .. } => (api_key, headers),
        crate::core::model_registry::ResolvedRequestAuth::Err { error } => {
            ctx.ui()?.notify(
                &format!("OpenAI remote compaction auth failed: {error}"),
                Some(NotifyType::Warning),
            );
            return Ok(None);
        }
    };
    let endpoint = build_compact_endpoint(&model);
    let mut messages = event.preparation.messages_to_summarize.clone();
    let turn_prefix_start = messages.len();
    messages.extend(event.preparation.turn_prefix_messages.clone());
    let pinned_message = if event.preparation.is_split_turn {
        event
            .preparation
            .turn_prefix_messages
            .iter()
            .position(|message| message.role() == "user")
            .map(|index| turn_prefix_start + index)
    } else {
        None
    };
    let instructions =
        build_openai_compact_instructions(&ctx.get_system_prompt()?, event.custom_instructions.as_deref());
    let payload = build_openai_compact_payload(
        &model,
        &messages,
        &instructions,
        &previous_replacement_history,
        pinned_message.into_iter().collect(),
    );
    let started_at = now_millis();
    let response = post_openai_compact(
        &endpoint,
        &payload,
        &model,
        api_key.as_deref(),
        headers.as_ref(),
        event.signal.clone(),
    )
    .await?;
    let duration_ms = now_millis() - started_at;
    let details = extract_openai_compact_details(&model, &response, &endpoint)?;
    Ok(Some(CompactionEventResult {
        cancel: None,
        compaction: Some(CompactionResult {
            summary: OPENAI_REMOTE_COMPACTION_SUMMARY.to_owned(),
            first_kept_entry_id: event.preparation.first_kept_entry_id.clone(),
            tokens_before: event.preparation.tokens_before,
            duration_ms: Some(duration_ms),
            estimated_tokens_after: None,
            kept_from_previous_context_tokens: None,
            compacted_result_bytes: Some(details.replacement_history_bytes),
            compacted_result_tokens: Some(details.replacement_history_tokens),
            source: Some(CompactionSourceInfo {
                r#type: CompactionSourceType::OpenAiRemote,
                provider: model.provider.clone(),
                model: model.id.clone(),
                endpoint: Some(endpoint),
            }),
            usage: None,
            provider_native: Some(ProviderNativeContext {
                provider: model.provider.clone(),
                api: model.api.clone(),
                format: ProviderNativeFormat::OpenAIResponsesInput,
                value: Value::Array(details.replacement_history.clone()),
            }),
            details: Some(serde_json::to_value(&details).map_err(|error| HandlerError::new(error.to_string()))?),
        }),
    }))
}

fn now_millis() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_millis() as f64)
}

/// `buildOpenAICompactInstructions`.
pub fn build_openai_compact_instructions(base_instructions: &str, custom_instructions: Option<&str>) -> String {
    let mut sections = vec![
        js_trim(base_instructions).to_owned(),
        OPENAI_REMOTE_COMPACTION_GUIDANCE.to_owned(),
    ];
    if let Some(custom_focus) = custom_instructions.map(js_trim).filter(|value| !value.is_empty()) {
        sections.push(format!(
            "Additional compaction focus requested by the user:\n{custom_focus}"
        ));
    }
    sections
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `isOpenAIResponsesModel`.
pub fn is_openai_responses_model(model: Option<&Model>) -> bool {
    let Some(model) = model else {
        return false;
    };
    if model.provider == "openai" && model.api == "openai-responses" {
        return true;
    }
    is_openai_codex_responses_model(Some(model))
}

fn is_openai_codex_responses_model(model: Option<&Model>) -> bool {
    model.is_some_and(|model| {
        (model.provider == "openai-codex" || model.provider == "openai-codex-gc")
            && model.api == "openai-codex-responses"
    })
}

fn is_codex_remote_compaction_model(model: &Model) -> bool {
    model.api == "openai-codex-responses" && (model.provider == "openai-codex" || model.provider == "openai-codex-gc")
}

fn get_openai_compact_model_id(model: &Model) -> String {
    if is_codex_remote_compaction_model(model) {
        CODEX_REMOTE_COMPACTION_MODEL.to_owned()
    } else {
        model.id.clone()
    }
}

/// `buildOpenAICompactPayload`.
pub fn build_openai_compact_payload(
    model: &Model,
    messages: &[pi_agent_core::agent::AgentMessage],
    instructions: &str,
    previous_replacement_history: &[Value],
    pinned_messages: HashSet<usize>,
) -> OpenAiCompactPayload {
    let converted_message_groups: Vec<(usize, Vec<Value>)> = messages
        .iter()
        .enumerate()
        .map(|(index, message)| (index, convert_agent_message_to_openai_response_items(model, message)))
        .collect();
    let mut converted_messages = Vec::new();
    let mut group_ranges = Vec::new();
    for (_, items) in &converted_message_groups {
        let start = converted_messages.len();
        converted_messages.extend(items.iter().cloned());
        group_ranges.push((start, converted_messages.len()));
    }
    let reconciled = reconcile_openai_tool_pairs(&converted_messages);
    let reconciled_set: HashSet<usize> = reconciled.iter().copied().collect();
    let mut pinned_raw_indices = Vec::new();
    for ((message_index, _), (start, end)) in converted_message_groups.iter().zip(group_ranges.iter()) {
        if pinned_messages.contains(message_index) {
            for index in *start..*end {
                if reconciled_set.contains(&index) {
                    pinned_raw_indices.push(index);
                }
            }
        }
    }
    let pinned_raw_items: Vec<Value> = pinned_raw_indices
        .iter()
        .map(|index| converted_messages[*index].clone())
        .collect();
    let bounded_pinned_raw_items =
        if serialized_openai_input_chars(&pinned_raw_items) <= MAX_OPENAI_REMOTE_COMPACT_CONTEXT_CHARS {
            pinned_raw_items
        } else {
            Vec::new()
        };
    let mut previous_with_pinned: Vec<Value> = previous_replacement_history.to_vec();
    previous_with_pinned.extend(bounded_pinned_raw_items.iter().cloned());
    let previous_history_fits =
        serialized_openai_input_chars(&previous_with_pinned) <= MAX_OPENAI_REMOTE_COMPACT_CONTEXT_CHARS;
    let encrypted_compaction_items: Vec<Value> = previous_replacement_history
        .iter()
        .filter(|item| is_openai_compaction_item(item))
        .cloned()
        .collect();
    let mut encrypted_with_pinned = encrypted_compaction_items.clone();
    encrypted_with_pinned.extend(bounded_pinned_raw_items.iter().cloned());
    let encrypted_compaction_items_fit =
        serialized_openai_input_chars(&encrypted_with_pinned) <= MAX_OPENAI_REMOTE_COMPACT_CONTEXT_CHARS;
    let pinned_previous_items: Vec<Value> = if previous_history_fits {
        previous_replacement_history.to_vec()
    } else if encrypted_compaction_items_fit {
        encrypted_compaction_items.clone()
    } else {
        Vec::new()
    };

    let mut items: Vec<Value> = previous_replacement_history.to_vec();
    items.extend(reconciled.iter().map(|index| converted_messages[*index].clone()));
    let native_items: HashSet<usize> = (0..previous_replacement_history.len()).collect();
    let mut pinned_items: HashSet<usize> = HashSet::new();
    if pinned_previous_items.len() == previous_replacement_history.len() {
        pinned_items.extend(0..previous_replacement_history.len());
    } else if !encrypted_compaction_items.is_empty() && pinned_previous_items.len() == encrypted_compaction_items.len()
    {
        for (index, item) in previous_replacement_history.iter().enumerate() {
            if is_openai_compaction_item(item) {
                pinned_items.insert(index);
            }
        }
    }
    let reconciled_position_by_index: HashMap<usize, usize> = reconciled
        .iter()
        .enumerate()
        .map(|(position, index)| (*index, position))
        .collect();
    let base = previous_replacement_history.len();
    for index in &pinned_raw_indices {
        if let Some(position) = reconciled_position_by_index.get(index) {
            pinned_items.insert(base + position);
        }
    }
    let input = limit_openai_compact_input(items, &pinned_items, &native_items);
    OpenAiCompactPayload {
        model: get_openai_compact_model_id(model),
        input,
        instructions: instructions.to_owned(),
        tools: Vec::new(),
        parallel_tool_calls: false,
    }
}

/// `extractOpenAICompactDetails`.
pub fn extract_openai_compact_details(
    model: &Model,
    response: &Value,
    endpoint: &str,
) -> Result<OpenAiRemoteCompactionDetails, HandlerError> {
    let Some(output) = response.get("output").and_then(Value::as_array) else {
        return Err(HandlerError::new(
            "OpenAI compact response did not include an output array",
        ));
    };
    let response_items: Vec<Value> = output.iter().filter(|item| is_record(item)).cloned().collect();
    if !response_items.iter().any(is_openai_compaction_item) {
        return Err(HandlerError::new(
            "OpenAI compact response did not include a compaction item",
        ));
    }
    let replacement_history = deduplicate_openai_message_items(response_items);
    let serialized = pi_js_compat::js_json::stringify(&Value::Array(replacement_history.clone()));
    let replacement_history_bytes = serialized.len() as f64;
    Ok(OpenAiRemoteCompactionDetails {
        details_type: DETAILS_TYPE.to_owned(),
        version: DETAILS_VERSION,
        provider: model.provider.clone(),
        api: model.api.clone(),
        model: get_openai_compact_model_id(model),
        endpoint: endpoint.to_owned(),
        replacement_history,
        replacement_history_bytes,
        replacement_history_tokens: (replacement_history_bytes / 4.0).ceil(),
    })
}

async fn post_openai_compact(
    endpoint: &str,
    payload: &OpenAiCompactPayload,
    model: &Model,
    api_key: Option<&str>,
    headers: Option<&ProviderHeaders>,
    signal: Option<tokio_util::sync::CancellationToken>,
) -> Result<Value, HandlerError> {
    let request_headers = build_openai_request_headers(model, api_key, headers)?;
    let body = serde_json::to_vec(payload).map_err(|error| HandlerError::new(error.to_string()))?;
    let fetch = pi_ai::http::default_fetch();
    let response = fetch
        .fetch(
            HttpRequest {
                method: "POST".to_owned(),
                url: endpoint.to_owned(),
                headers: request_headers,
                body: Some(body),
            },
            signal,
        )
        .await
        .map_err(|error| HandlerError::new(error.message))?;
    let status = response.status;
    let mut body = response.body;
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend(chunk.map_err(|error| HandlerError::new(error.message))?);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let parsed = parse_openai_compact_response(&text);
    if !(200..300).contains(&status) {
        return Err(HandlerError::new(format!(
            "OpenAI remote compaction failed ({status}): {}",
            format_openai_error(&parsed)
        )));
    }
    Ok(parsed)
}

fn parse_openai_compact_response(text: &str) -> Value {
    if text.is_empty() {
        return Value::Object(Map::new());
    }
    serde_json::from_str(text).unwrap_or_else(|_| json!({"output": [{"type": "error", "text": text}]}))
}

#[cfg(test)]
mod js_value_tests {
    use super::*;

    #[test]
    fn error_fallback_uses_javascript_json() {
        let body = serde_json::from_str(r#"{"2":2.0,"1":1.0}"#).unwrap();
        assert_eq!(format_openai_error(&body), r#"{"1":1,"2":2}"#);
    }

    #[test]
    fn serialized_budget_uses_javascript_numbers() {
        let items = vec![serde_json::from_str(r#"{"2":2.0,"1":1.0}"#).unwrap()];
        assert_eq!(serialized_openai_input_chars(&items), 15);
    }
}

fn format_openai_error(body: &Value) -> String {
    if let Some(error) = body.get("error")
        && let Some(message) = error.get("message").and_then(Value::as_str)
    {
        return message.to_owned();
    }
    pi_js_compat::js_json::stringify(body)
}

/// `buildCompactEndpoint`.
pub fn build_compact_endpoint(model: &Model) -> String {
    if model.api == "openai-codex-responses" {
        return format!("{}/compact", resolve_codex_responses_endpoint(&model.base_url));
    }
    format!("{}/responses/compact", model.base_url.trim_end_matches('/'))
}

/// `buildOpenAIRequestHeaders`.
pub fn build_openai_request_headers(
    model: &Model,
    api_key: Option<&str>,
    headers: Option<&ProviderHeaders>,
) -> Result<Vec<(String, String)>, HandlerError> {
    let mut request_headers: Vec<(String, String)> = vec![("content-type".to_owned(), "application/json".to_owned())];
    if let Some(headers) = headers {
        for (name, value) in headers {
            if let Some(value) = value {
                request_headers.push((name.clone(), value.clone()));
            }
        }
    }
    let has_authorization = request_headers
        .iter()
        .any(|(name, _)| name.to_lowercase() == "authorization");
    if let Some(api_key) = api_key
        && !has_authorization
    {
        request_headers.push(("Authorization".to_owned(), format!("Bearer {api_key}")));
    }
    if let Some(api_key) = api_key
        && model.api == "openai-codex-responses"
    {
        request_headers.push(("chatgpt-account-id".to_owned(), extract_codex_account_id(api_key)?));
        request_headers.push(("originator".to_owned(), "pi".to_owned()));
        request_headers.push(("OpenAI-Beta".to_owned(), "responses=experimental".to_owned()));
    }
    Ok(request_headers)
}

fn resolve_codex_responses_endpoint(base_url: &str) -> String {
    let normalized = base_url.trim_end_matches('/');
    if normalized.ends_with("/codex/responses") {
        normalized.to_owned()
    } else if normalized.ends_with("/codex") {
        format!("{normalized}/responses")
    } else {
        format!("{normalized}/codex/responses")
    }
}

fn extract_codex_account_id(token: &str) -> Result<String, HandlerError> {
    let failure = || HandlerError::new("Failed to extract accountId from Codex token");
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(failure());
    }
    let payload_bytes = buffer_from_base64(parts[1]);
    let payload = String::from_utf8_lossy(&payload_bytes);
    let payload: Value = serde_json::from_str(&payload).map_err(|_| failure())?;
    let auth = payload.get("https://api.openai.com/auth");
    let account_id = auth
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(Value::as_str);
    match account_id {
        Some(account_id) if !account_id.is_empty() => Ok(account_id.to_owned()),
        _ => Err(failure()),
    }
}

fn find_latest_openai_replacement_history(entries: &[SessionEntry], model: &Model) -> Vec<Value> {
    let Some(details) = find_latest_openai_replacement_history_details(entries) else {
        return Vec::new();
    };
    if details.provider != model.provider || details.api != model.api {
        return Vec::new();
    }
    if !is_codex_remote_compaction_model(model) && details.model != model.id {
        return Vec::new();
    }
    details.replacement_history
}

fn find_latest_openai_replacement_history_details(entries: &[SessionEntry]) -> Option<OpenAiRemoteCompactionDetails> {
    for entry in entries.iter().rev() {
        if let Some(compaction) = entry.as_compaction() {
            return compaction
                .details
                .as_deref()
                .and_then(is_openai_remote_compaction_details);
        }
    }
    None
}

fn is_openai_remote_compaction_details(value: &Value) -> Option<OpenAiRemoteCompactionDetails> {
    if value.get("type").and_then(Value::as_str) != Some(DETAILS_TYPE) {
        return None;
    }
    if value.get("version").and_then(Value::as_f64) != Some(DETAILS_VERSION) {
        return None;
    }
    let provider = value.get("provider")?.as_str()?;
    let api = value.get("api")?.as_str()?;
    if api != "openai-responses" && api != "openai-codex-responses" {
        return None;
    }
    let model = value.get("model")?.as_str()?;
    let endpoint = value.get("endpoint")?.as_str()?;
    let replacement_history = value.get("replacementHistory")?.as_array()?;
    if !replacement_history.iter().all(is_record) {
        return None;
    }
    let bytes = value.get("replacementHistoryBytes");
    if let Some(bytes) = bytes
        && bytes.is_number()
        && !bytes.as_f64().is_some_and(f64::is_finite)
    {
        return None;
    }
    let tokens = value.get("replacementHistoryTokens");
    if let Some(tokens) = tokens
        && tokens.is_number()
        && !tokens.as_f64().is_some_and(f64::is_finite)
    {
        return None;
    }
    Some(OpenAiRemoteCompactionDetails {
        details_type: DETAILS_TYPE.to_owned(),
        version: DETAILS_VERSION,
        provider: provider.to_owned(),
        api: api.to_owned(),
        model: model.to_owned(),
        endpoint: endpoint.to_owned(),
        replacement_history: replacement_history.clone(),
        replacement_history_bytes: value
            .get("replacementHistoryBytes")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        replacement_history_tokens: value
            .get("replacementHistoryTokens")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    })
}

fn deduplicate_openai_message_items(items: Vec<Value>) -> Vec<Value> {
    let keys: Vec<Option<String>> = items.iter().map(openai_message_deduplication_key).collect();
    let mut latest_index_by_key: HashMap<String, usize> = HashMap::new();
    for (index, key) in keys.iter().enumerate() {
        if let Some(key) = key {
            latest_index_by_key.insert(key.clone(), index);
        }
    }
    items
        .into_iter()
        .enumerate()
        .filter(|(index, _)| match &keys[*index] {
            Some(key) => latest_index_by_key.get(key) == Some(index),
            None => true,
        })
        .map(|(_, item)| item)
        .collect()
}

fn openai_message_deduplication_key(item: &Value) -> Option<String> {
    if item.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    item.get("id").and_then(Value::as_str)?;
    let mut message = item.as_object()?.clone();
    message.shift_remove("id");
    let canonical = canonicalize_json_value(&Value::Object(message));
    Some(pi_js_compat::js_json::stringify(&canonical))
}

fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json_value).collect()),
        Value::Object(object) => {
            let mut entries: Vec<(&String, &Value)> = object.iter().collect();
            entries.sort_by(|(left, _), (right, _)| locale_compare(left, right));
            let mut sorted = Map::new();
            for (key, child) in entries {
                sorted.insert(key.clone(), canonicalize_json_value(child));
            }
            Value::Object(sorted)
        }
        other => other.clone(),
    }
}

fn is_openai_compaction_item(value: &Value) -> bool {
    is_record(value)
        && matches!(
            value.get("type").and_then(Value::as_str),
            Some("compaction" | "compaction_summary")
        )
        && value.get("encrypted_content").and_then(Value::as_str).is_some()
}

fn convert_agent_message_to_openai_response_items(
    model: &Model,
    message: &pi_agent_core::agent::AgentMessage,
) -> Vec<Value> {
    convert_to_llm(std::slice::from_ref(message))
        .into_iter()
        .flat_map(|message| convert_message(model, &message))
        .collect()
}

fn convert_message(model: &Model, message: &Message) -> Vec<Value> {
    match message {
        Message::User(user) => convert_user_message(&user.content),
        Message::Assistant(assistant) => convert_assistant_message(&assistant.content),
        Message::ToolResult(result) => {
            vec![convert_tool_result_message(
                model,
                &result.tool_call_id,
                &result.content,
            )]
        }
    }
}

fn convert_user_message(content: &UserContent) -> Vec<Value> {
    let converted_content: Vec<Value> = match content {
        UserContent::Text(text) => vec![input_text(text)],
        UserContent::Blocks(blocks) => blocks.iter().map(convert_user_content_part).collect(),
    };
    if converted_content.is_empty() {
        return Vec::new();
    }
    vec![json!({"role": "user", "content": converted_content})]
}

fn convert_user_content_part(content: &TextOrImageContent) -> Value {
    match content {
        TextOrImageContent::Text(text) => input_text(&text.text),
        TextOrImageContent::Image(image) => json!({
            "type": "input_image",
            "detail": "auto",
            "image_url": format!("data:{};base64,{}", image.mime_type, image.data)
        }),
    }
}

fn convert_assistant_message(content: &[AssistantContent]) -> Vec<Value> {
    let mut output = Vec::new();
    for block in content {
        match block {
            AssistantContent::Text(text) => {
                output.push(json!({
                    "type": "message",
                    "role": "assistant",
                    "content": [output_text(&text.text)],
                    "status": "completed"
                }));
            }
            AssistantContent::ToolCall(tool_call) => {
                let mut parts = tool_call.id.split('|');
                let call_id = parts.next().unwrap_or_default();
                let item_id = parts.next();
                let mut item = Map::new();
                item.insert("type".to_owned(), Value::String("function_call".to_owned()));
                if let Some(item_id) = item_id {
                    item.insert("id".to_owned(), Value::String(item_id.to_owned()));
                }
                item.insert("call_id".to_owned(), Value::String(call_id.to_owned()));
                item.insert("name".to_owned(), Value::String(tool_call.name.clone()));
                item.insert(
                    "arguments".to_owned(),
                    Value::String(pi_js_compat::js_json::stringify(&Value::Object(
                        tool_call.arguments.clone(),
                    ))),
                );
                output.push(Value::Object(item));
            }
            _ => {}
        }
    }
    output
}

fn convert_tool_result_message(model: &Model, tool_call_id: &str, content: &[TextOrImageContent]) -> Value {
    let call_id = tool_call_id.split('|').next().unwrap_or_default();
    let text = content
        .iter()
        .filter_map(|part| match part {
            TextOrImageContent::Text(text) => Some(text.text.as_str()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let images: Vec<&pi_ai::types::ImageContent> = content
        .iter()
        .filter_map(|part| match part {
            TextOrImageContent::Image(image) => Some(image),
            TextOrImageContent::Text(_) => None,
        })
        .collect();
    if images.is_empty() {
        return json!({"type": "function_call_output", "call_id": call_id, "output": text});
    }
    if !model.input.contains(&Modality::Image) {
        let output = if text.is_empty() {
            "(see attached image)".to_owned()
        } else {
            text
        };
        return json!({"type": "function_call_output", "call_id": call_id, "output": output});
    }
    let mut output = Vec::new();
    if !text.is_empty() {
        output.push(input_text(&text));
    }
    for image in images {
        output.push(json!({
            "type": "input_image",
            "detail": "auto",
            "image_url": format!("data:{};base64,{}", image.mime_type, image.data)
        }));
    }
    json!({"type": "function_call_output", "call_id": call_id, "output": output})
}

fn input_text(text: &str) -> Value {
    json!({"type": "input_text", "text": text})
}

fn output_text(text: &str) -> Value {
    json!({"type": "output_text", "text": text})
}

fn is_record(value: &Value) -> bool {
    matches!(value, Value::Object(_) | Value::Array(_))
}

fn is_openai_text_part(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("input_text" | "output_text")
    ) && value.get("text").and_then(Value::as_str).is_some()
}

fn is_string_tool_output(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("function_call_output")
        && value.get("output").and_then(Value::as_str).is_some()
}

fn is_atomic_openai_item(item: &Value) -> bool {
    matches!(
        item.get("type").and_then(Value::as_str),
        Some("function_call" | "compaction" | "compaction_summary")
    )
}

fn is_image_part(value: &Value) -> bool {
    is_record(value) && value.get("type").and_then(Value::as_str) == Some("input_image")
}

fn is_function_call_item(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str) == Some("function_call")
        && item.get("call_id").and_then(Value::as_str).is_some()
}

fn is_function_call_output_item(item: &Value) -> bool {
    item.get("type").and_then(Value::as_str) == Some("function_call_output")
        && item.get("call_id").and_then(Value::as_str).is_some()
}

fn function_call_id(item: &Value) -> &str {
    item.get("call_id").and_then(Value::as_str).unwrap_or_default()
}

fn reconcile_openai_tool_pairs(items: &[Value]) -> Vec<usize> {
    let call_ids: HashSet<&str> = items
        .iter()
        .filter(|item| is_function_call_item(item))
        .map(function_call_id)
        .collect();
    let output_call_ids: HashSet<&str> = items
        .iter()
        .filter(|item| is_function_call_output_item(item))
        .map(function_call_id)
        .collect();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            if is_function_call_item(item) {
                return output_call_ids.contains(function_call_id(item));
            }
            if is_function_call_output_item(item) {
                return call_ids.contains(function_call_id(item));
            }
            true
        })
        .map(|(index, _)| index)
        .collect()
}

fn serialized_openai_input_chars(items: &[Value]) -> usize {
    let serialized = pi_js_compat::js_json::stringify(&Value::Array(items.to_vec()));
    utf16_len(&serialized)
}

/// `limitOpenAICompactInput`.
pub fn limit_openai_compact_input(
    items: Vec<Value>,
    pinned_items: &HashSet<usize>,
    native_items: &HashSet<usize>,
) -> Vec<Value> {
    if serialized_openai_input_chars(&items) <= MAX_OPENAI_REMOTE_COMPACT_CONTEXT_CHARS {
        return items;
    }
    let chunks = group_openai_tool_batches(&items, native_items);
    let mut limited_chunks: Vec<Option<Vec<Value>>> = chunks
        .iter()
        .map(|chunk| {
            if chunk.iter().any(|index| pinned_items.contains(index)) {
                Some(chunk.iter().map(|index| items[*index].clone()).collect())
            } else {
                None
            }
        })
        .collect();
    for index in (0..chunks.len()).rev() {
        if limited_chunks[index].is_some() {
            continue;
        }
        let chunk: Vec<Value> = chunks[index].iter().map(|item| items[*item].clone()).collect();
        let reserved_items = reserved_items(&limited_chunks);
        let mut with_reserved = chunk.clone();
        with_reserved.extend(reserved_items.iter().cloned());
        if serialized_openai_input_chars(&with_reserved) <= MAX_OPENAI_REMOTE_COMPACT_CONTEXT_CHARS {
            limited_chunks[index] = Some(chunk);
            continue;
        }
        if let Some(reduced) = reduce_openai_chunk_to_fit(&reserved_items, &chunk) {
            limited_chunks[index] = Some(reduced);
        }
    }
    limited_chunks.into_iter().flatten().flatten().collect()
}

fn reserved_items(limited_chunks: &[Option<Vec<Value>>]) -> Vec<Value> {
    limited_chunks
        .iter()
        .filter_map(|chunk| chunk.as_ref())
        .flatten()
        .cloned()
        .collect()
}

fn group_openai_tool_batches(items: &[Value], native_items: &HashSet<usize>) -> Vec<Vec<usize>> {
    let first_raw_item_index = items
        .iter()
        .enumerate()
        .position(|(index, _)| !native_items.contains(&index));
    let native_history_end_index = first_raw_item_index.unwrap_or(items.len());
    let mut chunks = Vec::new();
    let mut index = 0;
    while index < items.len() {
        if !is_function_call_item(&items[index]) {
            chunks.push(vec![index]);
            index += 1;
            continue;
        }
        let region_end_index = if index < native_history_end_index {
            native_history_end_index
        } else {
            items.len()
        };
        let batch_end_index = find_tool_batch_end_index(items, index, region_end_index);
        chunks.push((index..=batch_end_index).collect());
        index = batch_end_index + 1;
    }
    chunks
}

fn find_tool_batch_end_index(items: &[Value], start_index: usize, end_index: usize) -> usize {
    let mut pending_call_ids: HashSet<&str> = HashSet::new();
    let mut last_function_call_index = start_index;
    for (index, item) in items.iter().enumerate().take(end_index).skip(start_index) {
        if is_function_call_item(item) {
            pending_call_ids.insert(function_call_id(item));
            last_function_call_index = index;
            continue;
        }
        if is_function_call_output_item(item) {
            pending_call_ids.remove(function_call_id(item));
            if pending_call_ids.is_empty() {
                return index;
            }
        }
        if index > last_function_call_index && pending_call_ids.is_empty() {
            return last_function_call_index;
        }
    }
    last_function_call_index
}

fn reduce_openai_chunk_to_fit(reserved_items: &[Value], chunk: &[Value]) -> Option<Vec<Value>> {
    let without_images: Vec<Value> = chunk.iter().filter_map(drop_image_parts_from_item).collect();
    if without_images.is_empty() {
        return None;
    }
    let mut with_reserved = without_images.clone();
    with_reserved.extend(reserved_items.iter().cloned());
    if serialized_openai_input_chars(&with_reserved) <= MAX_OPENAI_REMOTE_COMPACT_CONTEXT_CHARS {
        return Some(without_images);
    }
    truncate_chunk_text_to_fit(reserved_items, &without_images)
}

fn drop_image_parts_from_item(item: &Value) -> Option<Value> {
    if is_atomic_openai_item(item) {
        return Some(item.clone());
    }
    if let Some(content) = item.get("content").and_then(Value::as_array) {
        let filtered: Vec<Value> = content.iter().filter(|part| !is_image_part(part)).cloned().collect();
        return if filtered.is_empty() {
            None
        } else {
            Some(replace_array_field(item, "content", filtered))
        };
    }
    if let Some(output) = item.get("output").and_then(Value::as_array) {
        let filtered: Vec<Value> = output.iter().filter(|part| !is_image_part(part)).cloned().collect();
        return if filtered.is_empty() {
            None
        } else {
            Some(replace_array_field(item, "output", filtered))
        };
    }
    Some(item.clone())
}

fn replace_array_field(item: &Value, field: &str, values: Vec<Value>) -> Value {
    let mut object = item.as_object().cloned().unwrap_or_default();
    object.insert(field.to_owned(), Value::Array(values));
    Value::Object(object)
}

fn truncate_chunk_text_to_fit(reserved_items: &[Value], chunk: &[Value]) -> Option<Vec<Value>> {
    let text_chars = count_reducible_text_chars(&Value::Array(chunk.to_vec()));
    let mut low = 0usize;
    let mut high = text_chars;
    let mut best: Option<Vec<Value>> = None;
    while low <= high {
        let mid = (low + high) / 2;
        let mut remaining = mid;
        let candidate = truncate_reducible_text(&Value::Array(chunk.to_vec()), &mut remaining);
        let Value::Array(candidate) = candidate else {
            return None;
        };
        if !candidate.iter().all(is_record) {
            return None;
        }
        let mut with_reserved = candidate.clone();
        with_reserved.extend(reserved_items.iter().cloned());
        if serialized_openai_input_chars(&with_reserved) <= MAX_OPENAI_REMOTE_COMPACT_CONTEXT_CHARS {
            best = Some(candidate);
            low = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }
    best.filter(|best| count_reducible_text_chars(&Value::Array(best.clone())) > 0)
}

fn count_reducible_text_chars(value: &Value) -> usize {
    if is_openai_text_part(value) {
        return value.get("text").and_then(Value::as_str).map_or(0, utf16_len);
    }
    if is_string_tool_output(value) {
        return value.get("output").and_then(Value::as_str).map_or(0, utf16_len);
    }
    if let Value::Array(items) = value {
        return items.iter().map(count_reducible_text_chars).sum();
    }
    if !is_record(value) || is_atomic_openai_item(value) {
        return 0;
    }
    match value {
        Value::Object(object) => object.values().map(count_reducible_text_chars).sum(),
        _ => 0,
    }
}

fn truncate_reducible_text(value: &Value, budget: &mut usize) -> Value {
    if is_openai_text_part(value) {
        let text = value.get("text").and_then(Value::as_str).unwrap_or_default();
        let kept_chars = utf16_len(text).min(*budget);
        *budget -= kept_chars;
        let mut object = value.as_object().cloned().unwrap_or_default();
        object.insert("text".to_owned(), Value::String(utf16_prefix(text, kept_chars)));
        return Value::Object(object);
    }
    if is_string_tool_output(value) {
        let output = value.get("output").and_then(Value::as_str).unwrap_or_default();
        let kept_chars = utf16_len(output).min(*budget);
        *budget -= kept_chars;
        let mut object = value.as_object().cloned().unwrap_or_default();
        object.insert("output".to_owned(), Value::String(utf16_prefix(output, kept_chars)));
        return Value::Object(object);
    }
    if let Value::Array(items) = value {
        return Value::Array(items.iter().map(|item| truncate_reducible_text(item, budget)).collect());
    }
    if !is_record(value) || is_atomic_openai_item(value) {
        return value.clone();
    }
    match value {
        Value::Object(object) => {
            let mut truncated = Map::new();
            for (key, item) in object {
                truncated.insert(key.clone(), truncate_reducible_text(item, budget));
            }
            Value::Object(truncated)
        }
        other => other.clone(),
    }
}

fn utf16_prefix(value: &str, units: usize) -> String {
    String::from_utf16_lossy(&value.encode_utf16().take(units).collect::<Vec<_>>())
}
