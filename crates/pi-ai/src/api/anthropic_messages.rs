//! Port of `packages/ai/src/api/anthropic-messages.ts`.
//!
//! The TypeScript module drives `@anthropic-ai/sdk`; the SDK's observable
//! request construction lives privately in [`super::anthropic_sdk`]. API-specific
//! options arrive through [`StreamOptions::extra`] and are parsed into
//! [`AnthropicOptions`]. A caller-supplied `client` is not representable in the
//! typed Rust contract.

use pi_js_compat::js_json::{JsString, JsValue};
use std::collections::BTreeSet;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::api::anthropic_sdk::{self, Auth, SdkRequest};
use crate::api::constrained_sampling::{get_json_schema_tool_parameters, resolve_json_schema_strict_sampling};
use crate::api::github_copilot_headers::{build_copilot_dynamic_headers, has_copilot_vision_input};
use crate::api::simple_options::{
    adjust_max_tokens_for_thinking, build_base_options, clamp_max_tokens_to_context, model_thinking_level,
};
use crate::api::transform_messages::transform_messages_with;
use crate::http::default_fetch;
use crate::models::calculate_cost;
use crate::request::{CallbackError, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis};
use crate::types::{
    AnthropicMessagesCompat, AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context,
    DoneReason, ErrorReason, JsonObject, Message, Model, ModelCompat, ModelThinkingLevel, ProviderHeaders,
    ProviderResponse, StopReason, TextContent, TextOrImageContent, ThinkingContent, ThinkingLevel, Tool, ToolCall,
    ToolResultMessage, UserContent,
};
use crate::utils::deferred_tools::{SplitDeferredTools, split_deferred_tools_with};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::headers::headers_to_record;
use crate::utils::json_parse::{parse_json_with_repair, parse_streaming_json};
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::provider_retry::{ProviderRequestError, ProviderRetryOptions, retry_provider_request};
use crate::utils::sanitize_unicode::sanitize_surrogates;
use pi_js_compat::number::js_number_value;

const FINE_GRAINED_TOOL_STREAMING_BETA: &str = "fine-grained-tool-streaming-2025-05-14";
const INTERLEAVED_THINKING_BETA: &str = "interleaved-thinking-2025-05-14";
const CLAUDE_CODE_VERSION: &str = "2.1.75";
const CLAUDE_CODE_IDENTITY: &str = "You are Claude Code, Anthropic's official CLI for Claude.";
const CLAUDE_CODE_TOOLS: [&str; 17] = [
    "Read",
    "Write",
    "Edit",
    "Bash",
    "Grep",
    "Glob",
    "AskUserQuestion",
    "EnterPlanMode",
    "ExitPlanMode",
    "KillShell",
    "NotebookEdit",
    "Skill",
    "Task",
    "TaskOutput",
    "TodoWrite",
    "WebFetch",
    "WebSearch",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnthropicEffort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnthropicThinkingDisplay {
    Summarized,
    Omitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuiltinToolChoice {
    Auto,
    Any,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceType {
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedToolChoice {
    #[serde(rename = "type")]
    pub kind: ToolChoiceType,
    pub name: String,
}

/// `"auto" | "any" | "none" | { type: "tool"; name }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicToolChoice {
    Named(NamedToolChoice),
    Builtin(BuiltinToolChoice),
}

/// `AnthropicOptions`: fields carried in [`StreamOptions::extra`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnthropicOptions {
    pub thinking_enabled: Option<bool>,
    pub thinking_budget_tokens: Option<f64>,
    pub effort: Option<AnthropicEffort>,
    pub thinking_display: Option<AnthropicThinkingDisplay>,
    pub interleaved_thinking: Option<bool>,
    pub tool_choice: Option<AnthropicToolChoice>,
}

impl AnthropicOptions {
    pub fn from_stream_options(options: &StreamOptions) -> Result<Self, String> {
        let extra = &options.extra;
        Ok(Self {
            thinking_enabled: optional(extra, "thinkingEnabled")?,
            thinking_budget_tokens: optional(extra, "thinkingBudgetTokens")?,
            effort: optional(extra, "effort")?,
            thinking_display: optional(extra, "thinkingDisplay")?,
            interleaved_thinking: optional(extra, "interleavedThinking")?,
            tool_choice: optional(extra, "toolChoice")?,
        })
    }
}

fn optional<T: serde::de::DeserializeOwned>(extra: &JsonObject, key: &str) -> Result<Option<T>, String> {
    match extra.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| format!("Invalid Anthropic option {key}: {error}")),
    }
}

#[derive(Debug, Clone, Copy)]
struct AnthropicCompat {
    supports_eager_tool_input_streaming: bool,
    supports_long_cache_retention: bool,
    send_session_affinity_headers: bool,
    supports_cache_control_on_tools: bool,
    supports_temperature: bool,
    allow_empty_signature: bool,
    supports_strict_tools: bool,
    supports_tool_references: bool,
}

fn anthropic_compat(model: &Model) -> AnthropicCompat {
    let compat = match &model.compat {
        Some(ModelCompat::AnthropicMessages(compat)) => Some(compat),
        _ => None,
    };
    let compat = compat.cloned().unwrap_or_default();
    AnthropicCompat {
        supports_eager_tool_input_streaming: compat.supports_eager_tool_input_streaming.unwrap_or(true),
        supports_long_cache_retention: compat.supports_long_cache_retention.unwrap_or(true),
        send_session_affinity_headers: compat.send_session_affinity_headers.unwrap_or(false),
        supports_cache_control_on_tools: compat.supports_cache_control_on_tools.unwrap_or(true),
        supports_temperature: compat.supports_temperature.unwrap_or(true),
        allow_empty_signature: compat.allow_empty_signature.unwrap_or(false),
        supports_strict_tools: compat.supports_strict_tools.unwrap_or(false),
        supports_tool_references: compat
            .supports_tool_references
            .unwrap_or_else(|| default_supports_tool_references(model)),
    }
}

fn raw_compat(model: &Model) -> AnthropicMessagesCompat {
    match &model.compat {
        Some(ModelCompat::AnthropicMessages(compat)) => compat.clone(),
        _ => AnthropicMessagesCompat::default(),
    }
}

fn default_supports_tool_references(model: &Model) -> bool {
    if model.provider != "anthropic" || model.id.contains("haiku") {
        return false;
    }
    let Some(rest) = model.id.strip_prefix("claude-") else {
        return false;
    };
    let Some(rest) = ["opus-", "sonnet-", "fable-"]
        .iter()
        .find_map(|prefix| rest.strip_prefix(prefix))
    else {
        return false;
    };
    let digits = |text: &str| text.chars().take_while(char::is_ascii_digit).count();
    let major_len = digits(rest);
    if major_len == 0 {
        return false;
    }
    let Ok(major) = rest[..major_len].parse::<u32>() else {
        return false;
    };
    let mut minor = 0;
    let after_major = &rest[major_len..];
    if let Some(after_dash) = after_major.strip_prefix('-') {
        let minor_len = digits(after_dash);
        let minor_text = &after_dash[..minor_len];
        let valid_tail = after_dash.len() == minor_len || after_dash[minor_len..].starts_with('-');
        if minor_len > 0 && minor_len < 8 && valid_tail {
            minor = minor_text.parse::<u32>().unwrap_or(0);
        }
    } else if !after_major.is_empty() {
        return false;
    }
    major > 4 || (major == 4 && minor >= 5)
}

fn to_claude_code_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    CLAUDE_CODE_TOOLS
        .iter()
        .find(|tool| tool.to_ascii_lowercase() == lower)
        .map_or_else(|| name.to_owned(), |tool| (*tool).to_owned())
}

fn from_claude_code_name(name: &str, tools: Option<&[Tool]>) -> String {
    let lower = name.to_ascii_lowercase();
    tools
        .into_iter()
        .flatten()
        .find(|tool| tool.name.to_ascii_lowercase() == lower)
        .map_or_else(|| name.to_owned(), |tool| tool.name.clone())
}

fn resolve_cache_retention(
    cache_retention: Option<CacheRetention>,
    env: Option<&crate::types::ProviderEnv>,
) -> CacheRetention {
    if let Some(cache_retention) = cache_retention {
        return cache_retention;
    }
    if get_provider_env_value("PI_CACHE_RETENTION", env).as_deref() == Some("long") {
        return CacheRetention::Long;
    }
    CacheRetention::Short
}

fn get_cache_control(
    model: &Model,
    cache_retention: Option<CacheRetention>,
    env: Option<&crate::types::ProviderEnv>,
) -> Option<Value> {
    let retention = resolve_cache_retention(cache_retention, env);
    if retention == CacheRetention::None {
        return None;
    }
    let ttl =
        (retention == CacheRetention::Long && anthropic_compat(model).supports_long_cache_retention).then_some("1h");
    let mut control = Map::new();
    control.insert("type".to_owned(), Value::String("ephemeral".to_owned()));
    if let Some(ttl) = ttl {
        control.insert("ttl".to_owned(), Value::String(ttl.to_owned()));
    }
    Some(Value::Object(control))
}

fn is_oauth_token(api_key: &str) -> bool {
    api_key.contains("sk-ant-oat")
}

fn js_trimmed_is_empty(value: impl Into<JsString>) -> bool {
    value.into().trim().is_empty()
}

fn has_header(headers: Option<&ProviderHeaders>, name: &str) -> bool {
    let Some(headers) = headers else {
        return false;
    };
    headers.iter().any(|(key, value)| {
        key.eq_ignore_ascii_case(name) && value.as_deref().is_some_and(|value| !js_trimmed_is_empty(value))
    })
}

fn assert_request_auth(provider: &str, api_key: Option<&str>, headers: Option<&ProviderHeaders>) -> Result<(), String> {
    if api_key.is_some_and(|api_key| !api_key.is_empty()) {
        return Ok(());
    }
    if has_header(headers, "authorization")
        || has_header(headers, "x-api-key")
        || has_header(headers, "cf-aig-authorization")
    {
        return Ok(());
    }
    Err(format!("No API key for provider: {provider}"))
}

fn provider_headers(headers: &IndexMap<String, String>) -> ProviderHeaders {
    headers
        .iter()
        .map(|(name, value)| (name.clone(), Some(value.clone())))
        .collect()
}

fn merge_headers(sources: &[&ProviderHeaders]) -> ProviderHeaders {
    let mut merged = ProviderHeaders::new();
    for source in sources {
        for (name, value) in *source {
            merged.insert(name.clone(), value.clone());
        }
    }
    merged
}

fn beta_header(features: &[&str]) -> Option<String> {
    (!features.is_empty()).then(|| features.join(","))
}

struct ClientHeaders {
    headers: ProviderHeaders,
    auth: Auth,
}

fn create_client_headers(
    model: &Model,
    context: &Context,
    options: &StreamOptions,
    anthropic: &AnthropicOptions,
    api_key: Option<&str>,
    is_oauth: bool,
    session_id: Option<&str>,
) -> ClientHeaders {
    let compat = anthropic_compat(model);
    let force_adaptive = raw_compat(model).force_adaptive_thinking == Some(true);
    let needs_interleaved_beta = anthropic.interleaved_thinking.unwrap_or(true) && !force_adaptive;
    let use_fine_grained =
        context.tools.as_ref().is_some_and(|tools| !tools.is_empty()) && !compat.supports_eager_tool_input_streaming;

    let mut beta_features = Vec::new();
    if use_fine_grained {
        beta_features.push(FINE_GRAINED_TOOL_STREAMING_BETA);
    }
    if needs_interleaved_beta {
        beta_features.push(INTERLEAVED_THINKING_BETA);
    }

    let empty_headers = ProviderHeaders::new();
    let options_headers = options.request.headers.as_ref().unwrap_or(&empty_headers);
    if model.provider == "github-copilot" {
        let dynamic = build_copilot_dynamic_headers(&context.messages, has_copilot_vision_input(&context.messages));
        let mut base = ProviderHeaders::new();
        base.insert("accept".to_owned(), Some("application/json".to_owned()));
        base.insert(
            "anthropic-dangerous-direct-browser-access".to_owned(),
            Some("true".to_owned()),
        );
        if let Some(beta) = beta_header(&beta_features) {
            base.insert("anthropic-beta".to_owned(), Some(beta));
        }
        let dynamic = provider_headers(&dynamic);
        let model_headers = model.headers.as_ref().map(provider_headers).unwrap_or_default();
        return ClientHeaders {
            headers: merge_headers(&[&base, &model_headers, &dynamic, options_headers]),
            auth: api_key.map_or(Auth::None, |api_key| Auth::Bearer(api_key.to_owned())),
        };
    }

    if is_oauth {
        let mut features = vec!["claude-code-20250219", "oauth-2025-04-20"];
        features.extend(beta_features);
        let mut base = ProviderHeaders::new();
        base.insert("accept".to_owned(), Some("application/json".to_owned()));
        base.insert(
            "anthropic-dangerous-direct-browser-access".to_owned(),
            Some("true".to_owned()),
        );
        base.insert("anthropic-beta".to_owned(), Some(features.join(",")));
        base.insert(
            "user-agent".to_owned(),
            Some(format!("claude-cli/{CLAUDE_CODE_VERSION}")),
        );
        base.insert("x-app".to_owned(), Some("cli".to_owned()));
        let model_headers = model.headers.as_ref().map(provider_headers).unwrap_or_default();
        return ClientHeaders {
            headers: merge_headers(&[&base, &model_headers, options_headers]),
            auth: api_key.map_or(Auth::None, |api_key| Auth::Bearer(api_key.to_owned())),
        };
    }

    let mut base = ProviderHeaders::new();
    base.insert("accept".to_owned(), Some("application/json".to_owned()));
    base.insert(
        "anthropic-dangerous-direct-browser-access".to_owned(),
        Some("true".to_owned()),
    );
    if let Some(beta) = beta_header(&beta_features) {
        base.insert("anthropic-beta".to_owned(), Some(beta));
    }
    if let (Some(session_id), true) = (session_id, compat.send_session_affinity_headers) {
        base.insert("x-session-affinity".to_owned(), Some(session_id.to_owned()));
    }
    let model_headers = model.headers.as_ref().map(provider_headers).unwrap_or_default();
    ClientHeaders {
        headers: merge_headers(&[&base, &model_headers, options_headers]),
        auth: api_key.map_or(Auth::None, |api_key| Auth::ApiKey(api_key.to_owned())),
    }
}

fn normalize_tool_call_id(id: &str, _model: &Model, _source: &AssistantMessage) -> String {
    normalize_tool_call_id_value(id)
}

fn normalize_tool_call_id_value(id: &str) -> String {
    let normalized: String = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    normalized.chars().take(64).collect()
}

fn convert_content_blocks(content: &[TextOrImageContent]) -> Value {
    let has_images = content
        .iter()
        .any(|block| matches!(block, TextOrImageContent::Image(_)));
    if !has_images {
        let joined = content
            .iter()
            .map(|block| match block {
                TextOrImageContent::Text(text) => sanitize_surrogates(&text.text),
                TextOrImageContent::Image(_) => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Value::String(sanitize_surrogates(&joined).to_owned());
    }

    let mut blocks: Vec<Value> = content
        .iter()
        .map(|block| match block {
            TextOrImageContent::Text(text) => {
                let mut object = Map::new();
                object.insert("type".to_owned(), Value::String("text".to_owned()));
                object.insert(
                    "text".to_owned(),
                    Value::String(sanitize_surrogates(&text.text).to_owned()),
                );
                Value::Object(object)
            }
            TextOrImageContent::Image(image) => {
                let mut source = Map::new();
                source.insert("type".to_owned(), Value::String("base64".to_owned()));
                source.insert("media_type".to_owned(), Value::String(image.mime_type.clone()));
                source.insert("data".to_owned(), Value::String(image.data.clone()));
                let mut object = Map::new();
                object.insert("type".to_owned(), Value::String("image".to_owned()));
                object.insert("source".to_owned(), Value::Object(source));
                Value::Object(object)
            }
        })
        .collect();
    let has_text = blocks.iter().any(|block| block["type"].as_str() == Some("text"));
    if !has_text {
        let mut placeholder = Map::new();
        placeholder.insert("type".to_owned(), Value::String("text".to_owned()));
        placeholder.insert("text".to_owned(), Value::String("(see attached image)".to_owned()));
        blocks.insert(0, Value::Object(placeholder));
    }
    Value::Array(blocks)
}

fn collect_deferred_tool_references(
    message: &ToolResultMessage,
    is_oauth: bool,
    deferred_tool_names: &BTreeSet<String>,
    loaded_tool_names: &mut BTreeSet<String>,
    normalize_tool_name: fn(&str) -> String,
) -> Vec<Value> {
    let mut references = Vec::new();
    for name in message.added_tool_names.iter().flatten() {
        let normalized = normalize_tool_name(name);
        if !deferred_tool_names.contains(&normalized) || loaded_tool_names.contains(&normalized) {
            continue;
        }
        loaded_tool_names.insert(normalized);
        let mut reference = Map::new();
        reference.insert("type".to_owned(), Value::String("tool_reference".to_owned()));
        reference.insert(
            "tool_name".to_owned(),
            Value::String(if is_oauth {
                to_claude_code_name(name)
            } else {
                name.clone()
            }),
        );
        references.push(Value::Object(reference));
    }
    references
}

fn convert_tool_result(
    message: &ToolResultMessage,
    is_oauth: bool,
    deferred_tool_names: &BTreeSet<String>,
    loaded_tool_names: &mut BTreeSet<String>,
    normalize_tool_name: fn(&str) -> String,
) -> (Value, Vec<Value>) {
    let references = collect_deferred_tool_references(
        message,
        is_oauth,
        deferred_tool_names,
        loaded_tool_names,
        normalize_tool_name,
    );
    let converted = convert_content_blocks(&message.content);
    let has_references = !references.is_empty();
    let sibling = if has_references {
        match &converted {
            Value::String(text) => {
                let mut block = Map::new();
                block.insert("type".to_owned(), Value::String("text".to_owned()));
                block.insert("text".to_owned(), Value::String(text.clone()));
                vec![Value::Object(block)]
            }
            Value::Array(blocks) => blocks.clone(),
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let mut tool_result = Map::new();
    tool_result.insert("type".to_owned(), Value::String("tool_result".to_owned()));
    tool_result.insert(
        "tool_use_id".to_owned(),
        Value::String(normalize_tool_call_id_value(&message.tool_call_id)),
    );
    tool_result.insert(
        "content".to_owned(),
        if has_references {
            Value::Array(references)
        } else {
            converted
        },
    );
    tool_result.insert("is_error".to_owned(), Value::Bool(message.is_error));
    (Value::Object(tool_result), sibling)
}

fn convert_messages(
    messages: &[Message],
    is_oauth: bool,
    cache_control: Option<&Value>,
    allow_empty_signature: bool,
    deferred_tool_names: &BTreeSet<String>,
    normalize_tool_name: fn(&str) -> String,
) -> Vec<Value> {
    let mut params = Vec::new();
    let mut loaded_tool_names = BTreeSet::new();
    let mut index = 0;
    while index < messages.len() {
        match &messages[index] {
            Message::User(message) => {
                push_user_message(&mut params, message);
            }
            Message::Assistant(message) => {
                if let Some(content) = convert_assistant_content(message, is_oauth, allow_empty_signature) {
                    let mut object = Map::new();
                    object.insert("role".to_owned(), Value::String("assistant".to_owned()));
                    object.insert("content".to_owned(), content);
                    params.push(Value::Object(object));
                }
            }
            Message::ToolResult(_) => {
                let mut tool_results = Vec::new();
                let mut sibling_content = Vec::new();
                while index < messages.len() {
                    let Message::ToolResult(message) = &messages[index] else {
                        break;
                    };
                    let (tool_result, sibling) = convert_tool_result(
                        message,
                        is_oauth,
                        deferred_tool_names,
                        &mut loaded_tool_names,
                        normalize_tool_name,
                    );
                    tool_results.push(tool_result);
                    sibling_content.extend(sibling);
                    index += 1;
                }
                let mut content = tool_results;
                content.extend(sibling_content);
                let mut object = Map::new();
                object.insert("role".to_owned(), Value::String("user".to_owned()));
                object.insert("content".to_owned(), Value::Array(content));
                params.push(Value::Object(object));
                continue;
            }
        }
        index += 1;
    }

    if let Some(cache_control) = cache_control
        && let Some(last_message) = params.last_mut()
        && last_message["role"].as_str() == Some("user")
    {
        let content = &mut last_message["content"];
        match content {
            Value::Array(blocks) => {
                if let Some(last_block) = blocks.last_mut()
                    && matches!(last_block["type"].as_str(), Some("text" | "image" | "tool_result"))
                {
                    last_block["cache_control"] = cache_control.clone();
                }
            }
            Value::String(text) => {
                let text = std::mem::take(text);
                let mut block = Map::new();
                block.insert("type".to_owned(), Value::String("text".to_owned()));
                block.insert("text".to_owned(), Value::String(text));
                block.insert("cache_control".to_owned(), cache_control.clone());
                *content = Value::Array(vec![Value::Object(block)]);
            }
            _ => {}
        }
    }
    params
}

fn push_user_message(params: &mut Vec<Value>, message: &crate::types::UserMessage) {
    let mut object = Map::new();
    object.insert("role".to_owned(), Value::String("user".to_owned()));
    match &message.content {
        UserContent::Text(text) => {
            if js_trimmed_is_empty(text) {
                return;
            }
            object.insert(
                "content".to_owned(),
                Value::String(sanitize_surrogates(text).to_owned()),
            );
        }
        UserContent::Blocks(blocks) => {
            let blocks: Vec<Value> = blocks
                .iter()
                .filter_map(|block| match block {
                    TextOrImageContent::Text(text) => {
                        if js_trimmed_is_empty(&text.text) {
                            return None;
                        }
                        let mut block = Map::new();
                        block.insert("type".to_owned(), Value::String("text".to_owned()));
                        block.insert(
                            "text".to_owned(),
                            Value::String(sanitize_surrogates(&text.text).to_owned()),
                        );
                        Some(Value::Object(block))
                    }
                    TextOrImageContent::Image(image) => {
                        let mut source = Map::new();
                        source.insert("type".to_owned(), Value::String("base64".to_owned()));
                        source.insert("media_type".to_owned(), Value::String(image.mime_type.clone()));
                        source.insert("data".to_owned(), Value::String(image.data.clone()));
                        let mut block = Map::new();
                        block.insert("type".to_owned(), Value::String("image".to_owned()));
                        block.insert("source".to_owned(), Value::Object(source));
                        Some(Value::Object(block))
                    }
                })
                .collect();
            if blocks.is_empty() {
                return;
            }
            object.insert("content".to_owned(), Value::Array(blocks));
        }
    }
    params.push(Value::Object(object));
}

fn convert_assistant_content(message: &AssistantMessage, is_oauth: bool, allow_empty_signature: bool) -> Option<Value> {
    let mut blocks = Vec::new();
    for block in &message.content {
        match block {
            AssistantContent::Text(text) => {
                if js_trimmed_is_empty(&text.text) {
                    continue;
                }
                let mut object = Map::new();
                object.insert("type".to_owned(), Value::String("text".to_owned()));
                object.insert(
                    "text".to_owned(),
                    Value::String(sanitize_surrogates(&text.text).to_owned()),
                );
                blocks.push(Value::Object(object));
            }
            AssistantContent::Thinking(thinking) => {
                if thinking.redacted == Some(true) {
                    let mut object = Map::new();
                    object.insert("type".to_owned(), Value::String("redacted_thinking".to_owned()));
                    if let Some(signature) = &thinking.thinking_signature {
                        object.insert("data".to_owned(), Value::String(signature.clone()));
                    }
                    blocks.push(Value::Object(object));
                    continue;
                }
                let signature = thinking.thinking_signature.as_deref();
                if js_trimmed_is_empty(&thinking.thinking) && signature.unwrap_or("").is_empty() {
                    continue;
                }
                if signature.is_none_or(js_trimmed_is_empty) {
                    if allow_empty_signature {
                        let mut object = Map::new();
                        object.insert("type".to_owned(), Value::String("thinking".to_owned()));
                        object.insert(
                            "thinking".to_owned(),
                            Value::String(sanitize_surrogates(&thinking.thinking).to_owned()),
                        );
                        object.insert("signature".to_owned(), Value::String(String::new()));
                        blocks.push(Value::Object(object));
                    } else {
                        let mut object = Map::new();
                        object.insert("type".to_owned(), Value::String("text".to_owned()));
                        object.insert(
                            "text".to_owned(),
                            Value::String(sanitize_surrogates(&thinking.thinking).to_owned()),
                        );
                        blocks.push(Value::Object(object));
                    }
                } else {
                    let mut object = Map::new();
                    object.insert("type".to_owned(), Value::String("thinking".to_owned()));
                    object.insert(
                        "thinking".to_owned(),
                        Value::String(sanitize_surrogates(&thinking.thinking).to_owned()),
                    );
                    object.insert(
                        "signature".to_owned(),
                        Value::String(signature.unwrap_or_default().to_owned()),
                    );
                    blocks.push(Value::Object(object));
                }
            }
            AssistantContent::ToolCall(call) => {
                let mut object = Map::new();
                object.insert("type".to_owned(), Value::String("tool_use".to_owned()));
                object.insert("id".to_owned(), Value::String(call.id.clone()));
                object.insert(
                    "name".to_owned(),
                    Value::String(if is_oauth {
                        to_claude_code_name(&call.name)
                    } else {
                        call.name.clone()
                    }),
                );
                object.insert("input".to_owned(), Value::Object(call.arguments.clone()));
                blocks.push(Value::Object(object));
            }
        }
    }
    (!blocks.is_empty()).then(|| Value::Array(blocks))
}

fn convert_tools(
    tools: &[Tool],
    is_oauth: bool,
    supports_eager_tool_input_streaming: bool,
    supports_strict_tools: bool,
    cache_control: Option<&Value>,
    defer_loading: bool,
) -> Result<Vec<Value>, String> {
    let mut converted = Vec::new();
    for (index, tool) in tools.iter().enumerate() {
        let strict =
            resolve_json_schema_strict_sampling(tool, supports_strict_tools).map_err(|error| error.message.clone())?;
        let parameters = get_json_schema_tool_parameters(tool, strict).map_err(|error| error.message.clone())?;
        let properties = parameters
            .get("properties")
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        let required = parameters
            .get("required")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let input_schema = if strict == Some(true) {
            let mut schema = parameters.as_object().cloned().unwrap_or_default();
            schema.insert("type".to_owned(), Value::String("object".to_owned()));
            schema.insert("properties".to_owned(), properties);
            schema.insert("required".to_owned(), required);
            Value::Object(schema)
        } else {
            let mut schema = Map::new();
            schema.insert("type".to_owned(), Value::String("object".to_owned()));
            schema.insert("properties".to_owned(), properties);
            schema.insert("required".to_owned(), required);
            Value::Object(schema)
        };

        let mut object = Map::new();
        object.insert(
            "name".to_owned(),
            Value::String(if is_oauth {
                to_claude_code_name(&tool.name)
            } else {
                tool.name.clone()
            }),
        );
        object.insert("description".to_owned(), Value::String(tool.description.clone()));
        if supports_eager_tool_input_streaming {
            object.insert("eager_input_streaming".to_owned(), Value::Bool(true));
        }
        if strict == Some(true) {
            object.insert("strict".to_owned(), Value::Bool(true));
        }
        object.insert("input_schema".to_owned(), input_schema);
        if defer_loading {
            object.insert("defer_loading".to_owned(), Value::Bool(true));
        }
        if let Some(cache_control) = cache_control
            && index == tools.len() - 1
        {
            object.insert("cache_control".to_owned(), cache_control.clone());
        }
        converted.push(Value::Object(object));
    }
    Ok(converted)
}

fn map_stop_reason(reason: &str, stop_details: Option<&Value>) -> Result<(StopReason, Option<String>), String> {
    match reason {
        "end_turn" => Ok((StopReason::Stop, None)),
        "max_tokens" => Ok((StopReason::Length, None)),
        "tool_use" => Ok((StopReason::ToolUse, None)),
        "refusal" => {
            let explanation = stop_details
                .and_then(|details| details.get("explanation"))
                .and_then(Value::as_str)
                .filter(|explanation| !explanation.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| "The model refused to complete the request".to_owned());
            Ok((StopReason::Error, Some(explanation)))
        }
        "pause_turn" | "stop_sequence" => Ok((StopReason::Stop, None)),
        "sensitive" => Ok((StopReason::Error, Some("Provider stopped with: sensitive".to_owned()))),
        other => Err(format!("Unhandled stop reason: {other}")),
    }
}

fn build_params(
    model: &Model,
    context: &Context,
    is_oauth: bool,
    anthropic: &AnthropicOptions,
    options: &StreamOptions,
) -> Result<Value, String> {
    let cache_control = get_cache_control(model, options.cache_retention, options.request.env.as_ref());
    let compat = anthropic_compat(model);
    let transformed = transform_messages_with(&context.messages, model, Some(&normalize_tool_call_id), now_millis());
    let normalize_tool_name: fn(&str) -> String = if is_oauth {
        to_claude_code_name
    } else {
        |name: &str| name.to_owned()
    };
    let (immediate, deferred_tools, deferred_tool_names) = split_tools(
        context,
        &transformed,
        compat.supports_tool_references,
        normalize_tool_name,
    );

    let mut params = Map::new();
    params.insert("model".to_owned(), Value::String(model.id.clone()));
    params.insert(
        "messages".to_owned(),
        Value::Array(convert_messages(
            &transformed,
            is_oauth,
            cache_control.as_ref(),
            compat.allow_empty_signature,
            &deferred_tool_names,
            normalize_tool_name,
        )),
    );
    params.insert(
        "max_tokens".to_owned(),
        js_number_value(options.max_tokens.unwrap_or(model.max_tokens)),
    );
    params.insert("stream".to_owned(), Value::Bool(true));

    insert_system_param(&mut params, context, is_oauth, cache_control.as_ref());
    insert_temperature_param(&mut params, options, anthropic, compat.supports_temperature);
    insert_tools_param(
        &mut params,
        &immediate,
        &deferred_tools,
        is_oauth,
        compat,
        cache_control.as_ref(),
    )?;
    insert_thinking_param(&mut params, model, anthropic);
    insert_metadata_param(&mut params, options);
    insert_tool_choice_param(&mut params, anthropic);

    Ok(Value::Object(params))
}

fn split_tools(
    context: &Context,
    transformed: &[Message],
    supports_tool_references: bool,
    normalize_tool_name: fn(&str) -> String,
) -> (Vec<Tool>, Vec<Tool>, BTreeSet<String>) {
    let transformed_context = Context {
        system_prompt: context.system_prompt.clone(),
        messages: transformed.to_vec(),
        tools: context.tools.clone(),
    };
    let SplitDeferredTools {
        mut immediate,
        deferred,
    } = split_deferred_tools_with(&transformed_context, supports_tool_references, normalize_tool_name);
    let mut deferred_tools: Vec<Tool> = deferred.into_iter().map(|(_, tool)| tool).collect();
    if immediate.is_empty() && !deferred_tools.is_empty() {
        immediate = std::mem::take(&mut deferred_tools);
    }
    let deferred_tool_names = deferred_tools
        .iter()
        .map(|tool| normalize_tool_name(&tool.name))
        .collect();
    (immediate, deferred_tools, deferred_tool_names)
}

fn insert_system_param(
    params: &mut Map<String, Value>,
    context: &Context,
    is_oauth: bool,
    cache_control: Option<&Value>,
) {
    let mut system = Vec::new();
    if is_oauth {
        system.push(text_block(CLAUDE_CODE_IDENTITY, cache_control));
    }
    if let Some(prompt) = &context.system_prompt {
        system.push(text_block(&sanitize_surrogates(prompt), cache_control));
    }
    if !system.is_empty() {
        params.insert("system".to_owned(), Value::Array(system));
    }
}

fn text_block(text: &str, cache_control: Option<&Value>) -> Value {
    let mut block = Map::new();
    block.insert("type".to_owned(), Value::String("text".to_owned()));
    block.insert("text".to_owned(), Value::String(text.to_owned()));
    if let Some(cache_control) = cache_control {
        block.insert("cache_control".to_owned(), cache_control.clone());
    }
    Value::Object(block)
}

fn insert_temperature_param(
    params: &mut Map<String, Value>,
    options: &StreamOptions,
    anthropic: &AnthropicOptions,
    supports_temperature: bool,
) {
    if let Some(temperature) = options.temperature
        && anthropic.thinking_enabled != Some(true)
        && supports_temperature
    {
        params.insert("temperature".to_owned(), js_number_value(temperature));
    }
}

fn insert_tools_param(
    params: &mut Map<String, Value>,
    immediate: &[Tool],
    deferred: &[Tool],
    is_oauth: bool,
    compat: AnthropicCompat,
    cache_control: Option<&Value>,
) -> Result<(), String> {
    if immediate.is_empty() && deferred.is_empty() {
        return Ok(());
    }
    let tool_cache_control = compat
        .supports_cache_control_on_tools
        .then_some(cache_control)
        .flatten();
    let mut tools = convert_tools(
        immediate,
        is_oauth,
        compat.supports_eager_tool_input_streaming,
        compat.supports_strict_tools,
        tool_cache_control,
        false,
    )?;
    tools.extend(convert_tools(
        deferred,
        is_oauth,
        compat.supports_eager_tool_input_streaming,
        compat.supports_strict_tools,
        None,
        true,
    )?);
    params.insert("tools".to_owned(), Value::Array(tools));
    Ok(())
}

fn insert_thinking_param(params: &mut Map<String, Value>, model: &Model, anthropic: &AnthropicOptions) {
    if !model.reasoning {
        return;
    }
    match anthropic.thinking_enabled {
        Some(true) => insert_enabled_thinking(params, model, anthropic),
        Some(false) if !thinking_off_is_null(model) => {
            let mut thinking = Map::new();
            thinking.insert("type".to_owned(), Value::String("disabled".to_owned()));
            params.insert("thinking".to_owned(), Value::Object(thinking));
        }
        _ => {}
    }
}

fn insert_enabled_thinking(params: &mut Map<String, Value>, model: &Model, anthropic: &AnthropicOptions) {
    let display = anthropic
        .thinking_display
        .unwrap_or(AnthropicThinkingDisplay::Summarized);
    let mut thinking = Map::new();
    if raw_compat(model).force_adaptive_thinking == Some(true) {
        thinking.insert("type".to_owned(), Value::String("adaptive".to_owned()));
        thinking.insert(
            "display".to_owned(),
            serde_json::to_value(display).unwrap_or(Value::Null),
        );
        params.insert("thinking".to_owned(), Value::Object(thinking));
        if let Some(effort) = anthropic.effort {
            let mut output_config = Map::new();
            output_config.insert("effort".to_owned(), serde_json::to_value(effort).unwrap_or(Value::Null));
            params.insert("output_config".to_owned(), Value::Object(output_config));
        }
        return;
    }
    thinking.insert("type".to_owned(), Value::String("enabled".to_owned()));
    let budget = anthropic
        .thinking_budget_tokens
        .filter(|budget| *budget != 0.0 && budget.is_finite())
        .unwrap_or(1024.0);
    thinking.insert("budget_tokens".to_owned(), js_number_value(budget));
    thinking.insert(
        "display".to_owned(),
        serde_json::to_value(display).unwrap_or(Value::Null),
    );
    params.insert("thinking".to_owned(), Value::Object(thinking));
}

fn thinking_off_is_null(model: &Model) -> bool {
    matches!(
        model
            .thinking_level_map
            .as_ref()
            .and_then(|map| map.get(&ModelThinkingLevel::Off)),
        Some(None)
    )
}

fn insert_metadata_param(params: &mut Map<String, Value>, options: &StreamOptions) {
    if let Some(metadata) = &options.metadata
        && let Some(user_id) = metadata.get("user_id").and_then(Value::as_str)
    {
        let mut object = Map::new();
        object.insert("user_id".to_owned(), Value::String(user_id.to_owned()));
        params.insert("metadata".to_owned(), Value::Object(object));
    }
}

fn insert_tool_choice_param(params: &mut Map<String, Value>, anthropic: &AnthropicOptions) {
    let Some(tool_choice) = &anthropic.tool_choice else {
        return;
    };
    let value = match tool_choice {
        AnthropicToolChoice::Builtin(builtin) => {
            let mut object = Map::new();
            object.insert("type".to_owned(), serde_json::to_value(builtin).unwrap_or(Value::Null));
            Value::Object(object)
        }
        AnthropicToolChoice::Named(named) => serde_json::to_value(named).unwrap_or(Value::Null),
    };
    params.insert("tool_choice".to_owned(), value);
}

fn find_block(output: &AssistantMessage, index: f64) -> Option<usize> {
    output.content.iter().position(|block| {
        let block_index = match block {
            AssistantContent::Text(block) => block.index,
            AssistantContent::Thinking(block) => block.index,
            AssistantContent::ToolCall(block) => block.index,
        };
        block_index == Some(index)
    })
}

fn event_index(event: &Value) -> Option<f64> {
    event.get("index").and_then(Value::as_f64)
}

fn number_field(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn update_usage_field(target: &mut f64, usage: &Value, key: &str) {
    if let Some(value) = number_field(usage, key) {
        *target = value;
    }
}

async fn drive_stream(
    sender: &AssistantMessageEventSender,
    model: &Model,
    context: &Context,
    options: &StreamOptions,
    output: &mut AssistantMessage,
) -> Result<(), String> {
    let anthropic = AnthropicOptions::from_stream_options(options)?;
    let api_key = options.request.api_key.as_deref().filter(|key| !key.is_empty());
    assert_request_auth(&model.provider, api_key, options.request.headers.as_ref())?;
    let is_oauth = api_key.is_some_and(is_oauth_token);
    let retention = resolve_cache_retention(options.cache_retention, options.request.env.as_ref());
    let session_id = (retention != CacheRetention::None)
        .then_some(options.session_id.as_deref())
        .flatten();
    let client = create_client_headers(model, context, options, &anthropic, api_key, is_oauth, session_id);
    let params = JsValue::from(build_params(model, context, is_oauth, &anthropic, options)?);
    let params = match &options.request.on_payload {
        Some(on_payload) => match on_payload(params.clone(), model.clone()).await {
            Ok(Some(next)) => next,
            Ok(None) => params,
            Err(error) => return Err(error.message),
        },
        None => params,
    };
    let request: SdkRequest =
        anthropic_sdk::build_request(model, &params, client.auth, &client.headers, options.request.timeout_ms)
            .map_err(|error| error.message)?;
    let fetch = options.request.fetch.clone().unwrap_or_else(default_fetch);
    let retry_options = ProviderRetryOptions {
        max_retries: max_retries(options.request.max_retries),
        max_retry_delay_ms: options.request.max_retry_delay_ms,
        signal: options.request.signal.clone(),
    };
    let signal = options.request.signal.clone();
    let response = retry_provider_request(
        || anthropic_sdk::execute(&request, Arc::clone(&fetch), signal.clone()),
        retry_options,
    )
    .await
    .map_err(provider_request_error_message)?;

    if let Some(on_response) = &options.request.on_response {
        let provider_response = ProviderResponse {
            status: f64::from(response.status),
            headers: headers_to_record(response.headers.iter().cloned()),
        };
        on_response(provider_response, model.clone())
            .await
            .map_err(|error| error.message)?;
    }

    sender.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });

    let mut reader = AnthropicEventReader::new(response.body, options.request.signal.clone());
    while let Some(event) = reader.next_event().await? {
        match event["type"].as_str() {
            Some("message_start") => apply_message_start(model, output, &event),
            Some("content_block_start") => apply_content_block_start(sender, context, output, &event, is_oauth),
            Some("content_block_delta") => apply_content_block_delta(sender, output, &event),
            Some("content_block_stop") => apply_content_block_stop(sender, output, &event),
            Some("message_delta") => apply_message_delta(model, output, &event)?,
            _ => {}
        }
    }
    reader.check_complete()?;

    if options
        .request
        .signal
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
    {
        return Err("Request was aborted".to_owned());
    }
    if output.stop_reason == StopReason::Pending {
        return Err("Anthropic stream ended without a stop reason".to_owned());
    }
    if matches!(output.stop_reason, StopReason::Aborted | StopReason::Error) {
        return Err(output
            .error_message
            .clone()
            .unwrap_or_else(|| "An unknown error occurred".to_owned()));
    }
    let reason = done_reason(output.stop_reason);
    sender.push(AssistantMessageEvent::Done {
        reason,
        message: output.clone(),
    });
    sender.end(Some(output.clone()));
    Ok(())
}

fn max_retries(value: Option<f64>) -> usize {
    value
        .filter(|value| *value > 0.0)
        .map(|value| value.floor() as usize)
        .unwrap_or(0)
}

fn provider_request_error_message(error: ProviderRequestError<CallbackError>) -> String {
    match error {
        ProviderRequestError::Provider(error) => error.message,
        ProviderRequestError::Other(error) => error.message,
        ProviderRequestError::Abort(_) => "Request aborted".to_owned(),
        ProviderRequestError::RetryDelayLimit(error) => error.to_string(),
    }
}

fn done_reason(reason: StopReason) -> DoneReason {
    match reason {
        StopReason::Length => DoneReason::Length,
        StopReason::ToolUse => DoneReason::ToolUse,
        StopReason::Deferred => DoneReason::Deferred,
        StopReason::Stop | StopReason::Pending | StopReason::Error | StopReason::Aborted => DoneReason::Stop,
    }
}

fn apply_message_start(model: &Model, output: &mut AssistantMessage, event: &Value) {
    let message = &event["message"];
    output.response_id = message["id"].as_str().map(str::to_owned);
    let usage = &message["usage"];
    output.usage.input = number_field(usage, "input_tokens").unwrap_or(0.0);
    output.usage.output = number_field(usage, "output_tokens").unwrap_or(0.0);
    output.usage.cache_read = number_field(usage, "cache_read_input_tokens").unwrap_or(0.0);
    output.usage.cache_write = number_field(usage, "cache_creation_input_tokens").unwrap_or(0.0);
    output.usage.cache_write_1h = Some(
        usage
            .get("cache_creation")
            .and_then(|creation| number_field(creation, "ephemeral_1h_input_tokens"))
            .unwrap_or(0.0),
    );
    update_total_and_cost(model, output);
}

fn update_total_and_cost(model: &Model, output: &mut AssistantMessage) {
    output.usage.total_tokens =
        output.usage.input + output.usage.output + output.usage.cache_read + output.usage.cache_write;
    output.usage.cost = calculate_cost(model, &output.usage);
}

fn apply_content_block_start(
    sender: &AssistantMessageEventSender,
    context: &Context,
    output: &mut AssistantMessage,
    event: &Value,
    is_oauth: bool,
) {
    let Some(index) = event_index(event) else {
        return;
    };
    let content_block = &event["content_block"];
    match content_block["type"].as_str() {
        Some("text") => {
            let text = content_block["text"].as_str().unwrap_or_default();
            let mut block = TextContent::new(text);
            block.index = Some(index);
            output.content.push(AssistantContent::Text(block));
            let content_index = output.content.len() - 1;
            sender.push(AssistantMessageEvent::TextStart {
                content_index,
                partial: output.clone(),
            });
        }
        Some("thinking") => {
            let mut thinking = ThinkingContent::new(content_block["thinking"].as_str().unwrap_or_default());
            thinking.thinking_signature = Some(content_block["signature"].as_str().unwrap_or_default().to_owned());
            thinking.index = Some(index);
            output.content.push(AssistantContent::Thinking(thinking));
            let content_index = output.content.len() - 1;
            sender.push(AssistantMessageEvent::ThinkingStart {
                content_index,
                partial: output.clone(),
            });
        }
        Some("redacted_thinking") => {
            let mut thinking = ThinkingContent::new("[Reasoning redacted]");
            thinking.thinking_signature = Some(content_block["data"].as_str().unwrap_or_default().to_owned());
            thinking.redacted = Some(true);
            thinking.index = Some(index);
            output.content.push(AssistantContent::Thinking(thinking));
            let content_index = output.content.len() - 1;
            sender.push(AssistantMessageEvent::ThinkingStart {
                content_index,
                partial: output.clone(),
            });
        }
        Some("tool_use") => {
            let name = content_block["name"].as_str().unwrap_or_default();
            let name = if is_oauth {
                from_claude_code_name(name, context.tools.as_deref())
            } else {
                name.to_owned()
            };
            let arguments = content_block["input"].as_object().cloned().unwrap_or_default();
            let id = content_block["id"].as_str().unwrap_or_default();
            let mut block = ToolCall::new(id, name, arguments);
            block.index = Some(index);
            block.partial_json = Some(String::new());
            output.content.push(AssistantContent::ToolCall(block));
            let content_index = output.content.len() - 1;
            sender.push(AssistantMessageEvent::ToolCallStart {
                content_index,
                partial: output.clone(),
            });
        }
        _ => {}
    }
}

fn apply_content_block_delta(sender: &AssistantMessageEventSender, output: &mut AssistantMessage, event: &Value) {
    let Some(index) = event_index(event) else {
        return;
    };
    let Some(position) = find_block(output, index) else {
        return;
    };
    let delta = &event["delta"];
    match delta["type"].as_str() {
        Some("text_delta") => {
            let text = delta["text"].as_str().unwrap_or_default();
            if let Some(AssistantContent::Text(block)) = output.content.get_mut(position) {
                block.text.push_str(text);
                sender.push(AssistantMessageEvent::TextDelta {
                    content_index: position,
                    delta: text.into(),
                    partial: output.clone(),
                });
            }
        }
        Some("thinking_delta") => {
            let thinking = delta["thinking"].as_str().unwrap_or_default();
            if let Some(AssistantContent::Thinking(block)) = output.content.get_mut(position) {
                block.thinking.push_str(thinking);
                sender.push(AssistantMessageEvent::ThinkingDelta {
                    content_index: position,
                    delta: thinking.to_owned(),
                    partial: output.clone(),
                });
            }
        }
        Some("input_json_delta") => {
            let partial_json = delta["partial_json"].as_str().unwrap_or_default();
            if let Some(AssistantContent::ToolCall(block)) = output.content.get_mut(position) {
                block.partial_json.get_or_insert_default().push_str(partial_json);
                block.arguments = parse_streaming_json(block.partial_json.as_deref())
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                sender.push(AssistantMessageEvent::ToolCallDelta {
                    content_index: position,
                    delta: partial_json.to_owned(),
                    partial: output.clone(),
                });
            }
        }
        Some("signature_delta") => {
            let signature = delta["signature"].as_str().unwrap_or_default();
            if let Some(AssistantContent::Thinking(block)) = output.content.get_mut(position) {
                let existing = block.thinking_signature.take().unwrap_or_default();
                block.thinking_signature = Some(existing + signature);
            }
        }
        _ => {}
    }
}

fn apply_content_block_stop(sender: &AssistantMessageEventSender, output: &mut AssistantMessage, event: &Value) {
    let Some(index) = event_index(event) else {
        return;
    };
    let Some(position) = find_block(output, index) else {
        return;
    };
    match output.content.get_mut(position) {
        Some(AssistantContent::Text(block)) => {
            block.index = None;
            sender.push(AssistantMessageEvent::TextEnd {
                content_index: position,
                content: block.text.clone(),
                partial: output.clone(),
            });
        }
        Some(AssistantContent::Thinking(block)) => {
            block.index = None;
            sender.push(AssistantMessageEvent::ThinkingEnd {
                content_index: position,
                content: block.thinking.clone(),
                partial: output.clone(),
            });
        }
        Some(AssistantContent::ToolCall(block)) => {
            block.index = None;
            block.arguments = parse_streaming_json(block.partial_json.take().as_deref())
                .as_object()
                .cloned()
                .unwrap_or_default();
            let tool_call = block.clone();
            sender.push(AssistantMessageEvent::ToolCallEnd {
                content_index: position,
                tool_call: Box::new(tool_call),
                partial: output.clone(),
            });
        }
        None => {}
    }
}

fn apply_message_delta(model: &Model, output: &mut AssistantMessage, event: &Value) -> Result<(), String> {
    let delta = &event["delta"];
    if let Some(stop_reason) = delta["stop_reason"].as_str() {
        output.raw_stop_reason = Some(stop_reason.to_owned());
        let (mapped, error_message) = map_stop_reason(stop_reason, delta.get("stop_details"))?;
        output.stop_reason = mapped;
        if let Some(error_message) = error_message {
            output.error_message = Some(error_message);
        }
    }
    if let Some(usage) = event.get("usage").filter(|usage| usage.is_object()) {
        update_usage_field(&mut output.usage.input, usage, "input_tokens");
        update_usage_field(&mut output.usage.output, usage, "output_tokens");
        update_usage_field(&mut output.usage.cache_read, usage, "cache_read_input_tokens");
        update_usage_field(&mut output.usage.cache_write, usage, "cache_creation_input_tokens");
        if let Some(thinking_tokens) = usage
            .get("output_tokens_details")
            .and_then(|details| number_field(details, "thinking_tokens"))
        {
            output.usage.reasoning = Some(thinking_tokens);
        }
    }
    update_total_and_cost(model, output);
    Ok(())
}

struct SseEvent {
    event: Option<String>,
    data: String,
    raw: Vec<String>,
}

#[derive(Default)]
struct SseState {
    event: Option<String>,
    data: Vec<String>,
    raw: Vec<String>,
}

impl SseState {
    fn flush(&mut self) -> Option<SseEvent> {
        if self.event.is_none() && self.data.is_empty() {
            return None;
        }
        Some(SseEvent {
            event: self.event.take(),
            data: std::mem::take(&mut self.data).join("\n"),
            raw: std::mem::take(&mut self.raw),
        })
    }

    fn decode_line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.flush();
        }
        self.raw.push(line.to_owned());
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.find(':') {
            Some(index) => (&line[..index], &line[index + 1..]),
            None => (line, ""),
        };
        let value = value.strip_prefix(' ').unwrap_or(value);
        if field == "event" {
            self.event = Some(value.to_owned());
        } else if field == "data" {
            self.data.push(value.to_owned());
        }
        None
    }
}

/// Incremental Anthropic SSE decoder shared with opt-in native provider examples.
pub struct AnthropicEventReader {
    body: Box<dyn crate::request::HttpBody>,
    signal: Option<CancellationToken>,
    queue: std::collections::VecDeque<SseEvent>,
    buffer: String,
    pending: Vec<u8>,
    state: SseState,
    finished: bool,
    saw_message_start: bool,
    saw_message_stop: bool,
}

impl AnthropicEventReader {
    pub fn new(body: Box<dyn crate::request::HttpBody>, signal: Option<CancellationToken>) -> Self {
        Self {
            body,
            signal,
            queue: std::collections::VecDeque::new(),
            buffer: String::new(),
            pending: Vec::new(),
            state: SseState::default(),
            finished: false,
            saw_message_start: false,
            saw_message_stop: false,
        }
    }

    pub async fn next_event(&mut self) -> Result<Option<Value>, String> {
        loop {
            if let Some(event) = self.queue.pop_front() {
                if let Some(event) = self.decode_event(event)? {
                    return Ok(Some(event));
                }
                continue;
            }
            if self.finished {
                return Ok(None);
            }
            if self.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err("Request was aborted".to_owned());
            }
            match self.body.next_chunk().await {
                None => {
                    self.finished = true;
                    let text = decode_available(&mut self.pending, true);
                    self.push_text(&text);
                    if !self.buffer.is_empty() {
                        let line = std::mem::take(&mut self.buffer);
                        if let Some(event) = self.state.decode_line(&line) {
                            self.queue.push_back(event);
                        }
                    }
                    if let Some(event) = self.state.flush() {
                        self.queue.push_back(event);
                    }
                }
                Some(Err(error)) => {
                    if self.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                        return Err("Request was aborted".to_owned());
                    }
                    return Err(error.message);
                }
                Some(Ok(bytes)) => {
                    self.pending.extend_from_slice(&bytes);
                    let text = decode_available(&mut self.pending, false);
                    self.push_text(&text);
                }
            }
        }
    }

    fn push_text(&mut self, text: &str) {
        self.buffer.push_str(text);
        loop {
            let Some((line_end, next)) = next_line_break(&self.buffer) else {
                return;
            };
            let line = self.buffer[..line_end].to_owned();
            self.buffer = self.buffer[next..].to_owned();
            if let Some(event) = self.state.decode_line(&line) {
                self.queue.push_back(event);
            }
        }
    }

    fn decode_event(&mut self, sse: SseEvent) -> Result<Option<Value>, String> {
        if sse.event.as_deref() == Some("error") {
            return Err(sse.data);
        }
        if !is_message_event(sse.event.as_deref()) {
            return Ok(None);
        }
        let event = parse_json_with_repair(&sse.data).map_err(|error| {
            format!(
                "Could not parse Anthropic SSE event {}: {}; data={}; raw={}",
                sse.event.as_deref().unwrap_or("null"),
                error.message(),
                sse.data,
                sse.raw.join("\\n")
            )
        })?;
        match event["type"].as_str() {
            Some("message_start") => self.saw_message_start = true,
            Some("message_stop") => self.saw_message_stop = true,
            _ => {}
        }
        Ok(Some(event))
    }

    fn check_complete(&self) -> Result<(), String> {
        if self.saw_message_start && !self.saw_message_stop {
            return Err("Anthropic stream ended before message_stop".to_owned());
        }
        Ok(())
    }
}

fn is_message_event(event: Option<&str>) -> bool {
    matches!(
        event,
        Some(
            "message_start"
                | "message_delta"
                | "message_stop"
                | "content_block_start"
                | "content_block_delta"
                | "content_block_stop"
        )
    )
}

fn next_line_break(text: &str) -> Option<(usize, usize)> {
    let carriage_return = text.find('\r');
    let newline = text.find('\n');
    let index = match (carriage_return, newline) {
        (None, None) => return None,
        (Some(index), None) | (None, Some(index)) => index,
        (Some(carriage), Some(newline)) => carriage.min(newline),
    };
    let mut next = index + 1;
    if text.as_bytes().get(index) == Some(&b'\r') && text.as_bytes().get(next) == Some(&b'\n') {
        next += 1;
    }
    Some((index, next))
}

fn decode_available(pending: &mut Vec<u8>, flush: bool) -> String {
    let mut output = String::new();
    loop {
        let (valid, error_len) = match std::str::from_utf8(pending) {
            Ok(text) => {
                let owned = text.to_owned();
                output.push_str(&owned);
                pending.clear();
                return output;
            }
            Err(error) => (error.valid_up_to(), error.error_len()),
        };
        if valid > 0 {
            let valid_text = std::str::from_utf8(&pending[..valid])
                .expect("valid_up_to prefix")
                .to_owned();
            output.push_str(&valid_text);
            pending.drain(..valid);
        }
        match error_len {
            Some(length) => {
                output.push('\u{fffd}');
                pending.drain(..length);
            }
            None => {
                if flush {
                    output.push('\u{fffd}');
                    pending.clear();
                }
                return output;
            }
        }
    }
}

pub fn stream(model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    tokio::spawn(run_stream(sender, model, context, options));
    stream
}

async fn run_stream(sender: AssistantMessageEventSender, model: Model, context: Context, options: StreamOptions) {
    let mut output = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis());
    let result = drive_stream(&sender, &model, &context, &options, &mut output).await;
    if let Err(message) = result {
        for block in &mut output.content {
            match block {
                AssistantContent::Text(block) => block.index = None,
                AssistantContent::Thinking(block) => block.index = None,
                AssistantContent::ToolCall(block) => {
                    block.index = None;
                    block.partial_json = None;
                }
            }
        }
        let aborted = options
            .request
            .signal
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled);
        output.stop_reason = if aborted {
            StopReason::Aborted
        } else {
            StopReason::Error
        };
        output.error_message = Some(message);
        sender.push(AssistantMessageEvent::Error {
            reason: if aborted {
                ErrorReason::Aborted
            } else {
                ErrorReason::Error
            },
            error: output.clone(),
        });
        sender.end(Some(output));
    }
}

fn map_thinking_level_to_effort(model: &Model, level: ThinkingLevel) -> AnthropicEffort {
    let mapped = model
        .thinking_level_map
        .as_ref()
        .and_then(|map| map.get(&model_thinking_level(level)))
        .and_then(|mapped| mapped.as_deref());
    if let Some(mapped) = mapped
        && let Ok(effort) = serde_json::from_value::<AnthropicEffort>(Value::String(mapped.to_owned()))
    {
        return effort;
    }
    match level {
        ThinkingLevel::Minimal | ThinkingLevel::Low => AnthropicEffort::Low,
        ThinkingLevel::Medium => AnthropicEffort::Medium,
        ThinkingLevel::High | ThinkingLevel::Xhigh | ThinkingLevel::Max | ThinkingLevel::Ultra => AnthropicEffort::High,
    }
}

fn with_extra(mut options: StreamOptions, key: &str, value: Value) -> StreamOptions {
    options.extra.insert(key.to_owned(), value);
    options
}

pub fn stream_simple(model: Model, context: Context, options: SimpleStreamOptions) -> AssistantMessageEventStream {
    let api_key = options.stream.request.api_key.as_deref().filter(|key| !key.is_empty());
    if let Err(message) = assert_request_auth(&model.provider, api_key, options.stream.request.headers.as_ref()) {
        return crate::request::error_stream(&model, &message);
    }

    let base = build_base_options(&model, &context, Some(&options), api_key);
    let Some(reasoning) = options.reasoning else {
        return stream(model, context, with_extra(base, "thinkingEnabled", Value::Bool(false)));
    };

    if raw_compat(&model).force_adaptive_thinking == Some(true) {
        let effort = map_thinking_level_to_effort(&model, reasoning);
        let base = with_extra(base, "thinkingEnabled", Value::Bool(true));
        let base = with_extra(base, "effort", serde_json::to_value(effort).unwrap_or(Value::Null));
        return stream(model, context, base);
    }

    let (adjusted_max_tokens, adjusted_budget) = adjust_max_tokens_for_thinking(
        base.max_tokens,
        model.max_tokens,
        reasoning,
        options.thinking_budgets.as_ref(),
    );
    let max_tokens = clamp_max_tokens_to_context(&model, &context, adjusted_max_tokens);
    let thinking_budget = adjusted_budget.min((max_tokens - 1024.0).max(0.0));
    let base = with_extra(base, "thinkingEnabled", Value::Bool(true));
    let mut base = with_extra(base, "thinkingBudgetTokens", js_number_value(thinking_budget));
    base.max_tokens = Some(max_tokens);
    stream(model, context, base)
}

/// `anthropicMessagesApi()` from the provider runtime: a unit value implementing
/// the uniform stream contract.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnthropicMessagesApi;

impl ProviderStreams for AnthropicMessagesApi {
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        stream(model, context, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        stream_simple(model, context, options)
    }
}

impl AnthropicMessagesApi {
    pub fn new() -> Self {
        Self
    }
}
