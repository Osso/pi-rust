//! Port of `packages/ai/src/api/openai-completions.ts` (+ `.lazy.ts`).

use crate::api::openai_sdk::ingress_metadata_text;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::{JsObject, JsValue};
use std::collections::BTreeMap;
use std::sync::Arc;

use indexmap::IndexMap;
use pi_js_compat::abort::AbortSignal;
use serde_json::{Map, Value};

use crate::api::github_copilot_headers::{build_copilot_dynamic_headers, has_copilot_vision_input};
use crate::api::lazy::{LazyApi, lazy_api};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_sdk::{SdkClient, SdkClientConfig, SdkError, format_request_error, format_sdk_error};
use crate::api::simple_options::{build_base_options, clamp_standard_reasoning_level, model_thinking_level};
use crate::api::transform_messages::transform_messages_with;
use crate::models::{calculate_cost, clamp_thinking_level};
use crate::request::{
    BoxFuture, CallbackError, ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis,
};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheControlFormat, CacheRetention, CompatFields,
    Context, DoneReason, ErrorReason, ImageContent, MaxTokensField, Message, Model, ModelCompat, ModelThinkingLevel,
    OpenAICompletionsCompat, ProviderEnv, ProviderHeaders, ProviderResponse, StopReason, TextContent, ThinkingBudgets,
    ThinkingContent, ThinkingFormat, Tool, ToolCall, Usage, UsageCost, spread_compat_properties,
};
use crate::utils::event_stream::{AssistantMessageEventSender, create_assistant_message_event_stream};
use crate::utils::hash::short_hash;
use crate::utils::headers::headers_to_record;
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::provider_retry::{ProviderRequestError, ProviderRetryOptions, retry_provider_request};
use crate::utils::sanitize_unicode::sanitize_surrogates;
use pi_js_compat::number::js_number_value;

/// `OpenAICompletionsOptions`.
#[derive(Debug, Clone, Default)]
pub struct OpenAICompletionsOptions {
    pub tool_choice: Option<Value>,
    pub reasoning_effort: Option<String>,
    pub thinking_budgets: Option<ThinkingBudgets>,
}

impl OpenAICompletionsOptions {
    pub fn from_stream_options(options: &StreamOptions) -> Self {
        Self {
            tool_choice: options.extra.get("toolChoice").cloned(),
            reasoning_effort: options
                .extra
                .get("reasoningEffort")
                .and_then(Value::as_str)
                .map(str::to_owned),
            thinking_budgets: options
                .extra
                .get("thinkingBudgets")
                .and_then(|value| serde_json::from_value(value.clone()).ok()),
        }
    }
}

/// Resolved `OpenAICompletionsCompat` after provider/URL detection.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedOpenAICompletionsCompat {
    pub supports_store: bool,
    pub supports_developer_role: bool,
    pub supports_reasoning_effort: bool,
    pub supports_usage_in_streaming: bool,
    pub max_tokens_field: MaxTokensField,
    pub requires_tool_result_name: bool,
    pub requires_assistant_after_tool_result: bool,
    pub requires_thinking_as_text: bool,
    pub requires_reasoning_content_on_assistant_messages: bool,
    pub thinking_format: ThinkingFormat,
    pub chat_template_kwargs: JsValue,
    pub zai_tool_stream: bool,
    pub supports_strict_mode: bool,
    pub cache_control_format: Option<CacheControlFormat>,
    pub send_session_affinity_headers: bool,
    pub supports_long_cache_retention: bool,
}

fn resolve_cache_retention(cache_retention: Option<CacheRetention>, env: Option<&ProviderEnv>) -> CacheRetention {
    if let Some(cache_retention) = cache_retention {
        return cache_retention;
    }
    if get_provider_env_value("PI_CACHE_RETENTION", env).is_some_and(|value| value == "long") {
        return CacheRetention::Long;
    }
    CacheRetention::Short
}

fn detect_compat(model: &Model) -> ResolvedOpenAICompletionsCompat {
    let provider = model.provider.as_str();
    let base_url = &model.base_url;
    let lower_base_url = base_url.to_lowercase();

    let is_zai = provider == "zai"
        || provider == "zai-coding-cn"
        || base_url.contains("api.z.ai")
        || base_url.contains("open.bigmodel.cn");
    let is_together =
        provider == "together" || base_url.contains("api.together.ai") || base_url.contains("api.together.xyz");
    let is_moonshot = provider == "moonshotai" || provider == "moonshotai-cn" || base_url.contains("api.moonshot.");
    let is_open_router = provider == "openrouter" || base_url.contains("openrouter.ai");
    let is_cloudflare_workers_ai = provider == "cloudflare-workers-ai" || base_url.contains("api.cloudflare.com");
    let is_cloudflare_ai_gateway =
        provider == "cloudflare-ai-gateway" || base_url.contains("gateway.ai.cloudflare.com");
    let is_nvidia = provider == "nvidia" || base_url.contains("integrate.api.nvidia.com");
    let is_ant_ling = provider == "ant-ling" || base_url.contains("api.ant-ling.com");
    let is_deep_seek = provider == "deepseek" || lower_base_url.contains("deepseek.com");

    let is_non_standard = is_nvidia
        || provider == "cerebras"
        || base_url.contains("cerebras.ai")
        || provider == "xai"
        || base_url.contains("api.x.ai")
        || is_together
        || base_url.contains("chutes.ai")
        || is_deep_seek
        || is_zai
        || is_moonshot
        || provider == "opencode"
        || base_url.contains("opencode.ai")
        || is_cloudflare_workers_ai
        || is_cloudflare_ai_gateway
        || is_ant_ling;

    let use_max_tokens = base_url.contains("chutes.ai")
        || is_moonshot
        || is_cloudflare_ai_gateway
        || is_together
        || is_nvidia
        || is_ant_ling;

    let is_grok = provider == "xai" || base_url.contains("api.x.ai");
    let is_open_router_developer_role_model =
        is_open_router && (model.id.starts_with("anthropic/") || model.id.starts_with("openai/"));
    let cache_control_format =
        (provider == "openrouter" && model.id.starts_with("anthropic/")).then_some(CacheControlFormat::Anthropic);

    ResolvedOpenAICompletionsCompat {
        supports_store: !is_non_standard,
        supports_developer_role: is_open_router_developer_role_model || (!is_non_standard && !is_open_router),
        supports_reasoning_effort: !is_grok
            && !is_zai
            && !is_moonshot
            && !is_together
            && !is_cloudflare_ai_gateway
            && !is_nvidia
            && !is_ant_ling,
        supports_usage_in_streaming: true,
        max_tokens_field: if use_max_tokens {
            MaxTokensField::MaxTokens
        } else {
            MaxTokensField::MaxCompletionTokens
        },
        requires_tool_result_name: false,
        requires_assistant_after_tool_result: false,
        requires_thinking_as_text: false,
        requires_reasoning_content_on_assistant_messages: is_deep_seek,
        thinking_format: if is_deep_seek {
            ThinkingFormat::Deepseek
        } else if is_zai {
            ThinkingFormat::Zai
        } else if is_together {
            ThinkingFormat::Together
        } else if is_ant_ling {
            ThinkingFormat::AntLing
        } else if is_open_router {
            ThinkingFormat::Openrouter
        } else {
            ThinkingFormat::Openai
        },
        chat_template_kwargs: JsValue::Object(JsObject::new()),
        zai_tool_stream: false,
        supports_strict_mode: !is_moonshot && !is_together && !is_cloudflare_ai_gateway && !is_nvidia,
        cache_control_format,
        send_session_affinity_headers: false,
        supports_long_cache_retention: !(is_together
            || is_cloudflare_workers_ai
            || is_cloudflare_ai_gateway
            || is_nvidia
            || is_ant_ling),
    }
}

pub fn get_compat(model: &Model) -> ResolvedOpenAICompletionsCompat {
    let detected = detect_compat(model);
    let Some(ModelCompat::OpenAICompletions(compat)) = &model.compat else {
        return detected;
    };
    // Frozen1271-1291 resolves nullish values; each accessor below implements
    // the subsequent literal read (truthiness, !==false, or strict equality).
    ResolvedOpenAICompletionsCompat {
        supports_store: compat.truthy("supportsStore", detected.supports_store), // :566
        supports_developer_role: compat.truthy("supportsDeveloperRole", detected.supports_developer_role), // :873
        supports_reasoning_effort: compat.truthy("supportsReasoningEffort", detected.supports_reasoning_effort), // :606/666
        supports_usage_in_streaming: compat.not_false("supportsUsageInStreaming", detected.supports_usage_in_streaming), // :562
        max_tokens_field: if compat.string_is(
            "maxTokensField",
            "max_tokens",
            Some(match detected.max_tokens_field {
                MaxTokensField::MaxTokens => "max_tokens",
                MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
            }),
        ) {
            MaxTokensField::MaxTokens
        } else {
            MaxTokensField::MaxCompletionTokens
        }, // :571-575
        requires_tool_result_name: compat.truthy("requiresToolResultName", detected.requires_tool_result_name), // :1042
        requires_assistant_after_tool_result: compat.truthy(
            "requiresAssistantAfterToolResult",
            detected.requires_assistant_after_tool_result,
        ), // :884/923/1064
        requires_thinking_as_text: compat.truthy("requiresThinkingAsText", detected.requires_thinking_as_text), // :942
        requires_reasoning_content_on_assistant_messages: compat.truthy(
            "requiresReasoningContentOnAssistantMessages",
            detected.requires_reasoning_content_on_assistant_messages,
        ), // :1001
        thinking_format: select_thinking_format(compat, detected.thinking_format),
        chat_template_kwargs: compat.nullish_default("chatTemplateKwargs", detected.chat_template_kwargs), // :1286/702
        zai_tool_stream: compat.truthy("zaiToolStream", detected.zai_tool_stream),                         // :584
        supports_strict_mode: compat.not_false("supportsStrictMode", detected.supports_strict_mode),       // :1105
        cache_control_format: compat
            .string_is(
                "cacheControlFormat",
                "anthropic",
                detected.cache_control_format.map(|_| "anthropic"),
            )
            .then_some(CacheControlFormat::Anthropic), // :737
        send_session_affinity_headers: compat
            .truthy("sendSessionAffinityHeaders", detected.send_session_affinity_headers), // :521
        supports_long_cache_retention: compat
            .truthy("supportsLongCacheRetention", detected.supports_long_cache_retention), // :556/559/741
    }
}

fn select_thinking_format(compat: &OpenAICompletionsCompat, detected: ThinkingFormat) -> ThinkingFormat {
    let value = compat.nullish_default(
        "thinkingFormat",
        pi_js_compat::js_json::to_value(&detected).expect("thinking format serializes"),
    );
    // Frozen600-675 consists of strict string comparisons. Any other JSON
    // value reaches the generic reasoning-effort branch, just like "openai".
    match value.as_string() {
        Some(value) if value == "zai" => ThinkingFormat::Zai,
        Some(value) if value == "qwen" => ThinkingFormat::Qwen,
        Some(value) if value == "qwen-chat-template" => ThinkingFormat::QwenChatTemplate,
        Some(value) if value == "chat-template" => ThinkingFormat::ChatTemplate,
        Some(value) if value == "deepseek" => ThinkingFormat::Deepseek,
        Some(value) if value == "openrouter" => ThinkingFormat::Openrouter,
        Some(value) if value == "ant-ling" => ThinkingFormat::AntLing,
        Some(value) if value == "together" => ThinkingFormat::Together,
        Some(value) if value == "string-thinking" => ThinkingFormat::StringThinking,
        _ => ThinkingFormat::Openai,
    }
}

fn has_header(headers: Option<&ProviderHeaders>, name: &str) -> bool {
    headers.is_some_and(|headers| {
        headers.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case(name) && value.as_ref().is_some_and(|value| !value.trim().is_empty())
        })
    })
}

fn get_client_api_key(
    provider: &str,
    api_key: Option<&JsString>,
    headers: Option<&ProviderHeaders>,
) -> Result<JsString, String> {
    if let Some(api_key) = api_key.filter(|api_key| !api_key.is_empty()) {
        return Ok(api_key.to_owned());
    }
    if has_header(headers, "authorization") || has_header(headers, "cf-aig-authorization") {
        return Ok("unused".into());
    }
    Err(format!("No API key for provider: {provider}"))
}

fn has_tool_history(messages: &[Message]) -> bool {
    messages.iter().any(|message| match message {
        Message::ToolResult(_) => true,
        Message::Assistant(assistant) => assistant
            .content
            .iter()
            .any(|block| matches!(block, AssistantContent::ToolCall(_))),
        Message::User(_) => false,
    })
}

// =============================================================================
// Message conversion
// =============================================================================

fn normalize_tool_call_id(id: &str, model: &Model) -> String {
    if let Some(separator_index) = id.find('|') {
        let call_id = sanitize_id_part(&id[..separator_index]);
        let item_id = sanitize_id_part(&id[separator_index + 1..]);
        let combined_id = if item_id.is_empty() {
            call_id.clone()
        } else {
            format!("{call_id}_{item_id}")
        };
        if combined_id.len() <= 40 {
            return combined_id;
        }
        let hash = short_hash(id).chars().take(8).collect::<String>();
        let prefix_length = 40_usize.saturating_sub(hash.len()).saturating_sub(1).max(1);
        let prefix = call_id.chars().take(prefix_length).collect::<String>();
        return format!("{prefix}_{hash}");
    }
    if model.provider == "openai" && id.len() > 40 {
        return id.chars().take(40).collect();
    }
    id.to_owned()
}

fn sanitize_id_part(part: &str) -> String {
    part.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn lossless_object(entries: Vec<(&str, JsValue)>) -> JsValue {
    JsValue::object(entries)
}

pub fn convert_messages(model: &Model, context: &Context, compat: &ResolvedOpenAICompletionsCompat) -> Vec<JsValue> {
    let mut params = Vec::new();

    let normalizer = |id: &str, target_model: &Model, _source: &AssistantMessage| -> String {
        normalize_tool_call_id(id, target_model)
    };
    let transformed_messages =
        transform_messages_with(&context.messages, model, Some(&normalizer), now_millis() as f64);

    if let Some(system_prompt) = context.system_prompt.as_ref() {
        let role = if model.reasoning && compat.supports_developer_role {
            "developer"
        } else {
            "system"
        };
        params.push(lossless_object(vec![
            ("role", JsValue::String(role.into())),
            ("content", JsValue::String(sanitize_surrogates(system_prompt).into())),
        ]));
    }

    let mut last_role: Option<String> = None;
    let mut index = 0;
    while index < transformed_messages.len() {
        let msg = &transformed_messages[index];
        if compat.requires_assistant_after_tool_result
            && last_role.as_deref() == Some("toolResult")
            && matches!(msg, Message::User(_))
        {
            params.push(lossless_object(vec![
                ("role", JsValue::String("assistant".into())),
                ("content", JsValue::String("I have processed the tool results.".into())),
            ]));
        }

        match msg {
            Message::User(user) => {
                match &user.content {
                    crate::types::UserContent::Text(text) => {
                        params.push(lossless_object(vec![
                            ("role", JsValue::String("user".into())),
                            ("content", JsValue::String(sanitize_surrogates(text).into())),
                        ]));
                    }
                    crate::types::UserContent::Blocks(blocks) => {
                        let content = blocks
                            .iter()
                            .map(|block| match block {
                                crate::types::TextOrImageContent::Text(text) => lossless_object(vec![
                                    ("type", JsValue::String("text".into())),
                                    ("text", JsValue::String(sanitize_surrogates(&text.text).into())),
                                ]),
                                crate::types::TextOrImageContent::Image(image) => lossless_object(vec![
                                    ("type", JsValue::String("image_url".into())),
                                    (
                                        "image_url",
                                        lossless_object(vec![(
                                            "url",
                                            JsValue::String(
                                                format!("data:{};base64,{}", image.mime_type, image.data).into(),
                                            ),
                                        )]),
                                    ),
                                ]),
                            })
                            .collect::<Vec<_>>();
                        if content.is_empty() {
                            index += 1;
                            continue;
                        }
                        params.push(lossless_object(vec![
                            ("role", JsValue::String("user".into())),
                            ("content", JsValue::Array(content)),
                        ]));
                    }
                }
                last_role = Some("user".into());
            }
            Message::Assistant(assistant) => {
                let assistant = assistant.as_ref();
                let mut assistant_message = JsObject::new();
                assistant_message.insert("role".into(), JsValue::String("assistant".into()));
                assistant_message.insert(
                    "content".into(),
                    if compat.requires_assistant_after_tool_result {
                        JsValue::String(String::new().into())
                    } else {
                        JsValue::Null
                    },
                );

                let assistant_text_parts = assistant
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        AssistantContent::Text(text) if !text.text.trim().is_empty() => Some(lossless_object(vec![
                            ("type", JsValue::String("text".into())),
                            ("text", JsValue::String(sanitize_surrogates(&text.text).into())),
                        ])),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let assistant_text = pi_js_compat::js_json::JsString::join(
                    assistant_text_parts
                        .iter()
                        .filter_map(|part| part.get("text").and_then(JsValue::as_string)),
                    "",
                );

                let non_empty_thinking = assistant
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        AssistantContent::Thinking(thinking) if !thinking.thinking.trim().is_empty() => Some(thinking),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if !non_empty_thinking.is_empty() {
                    if compat.requires_thinking_as_text {
                        let thinking_text = non_empty_thinking
                            .iter()
                            .map(|block| sanitize_surrogates(&block.thinking))
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        let mut content = vec![lossless_object(vec![
                            ("type", JsValue::String("text".into())),
                            ("text", JsValue::String(thinking_text.into())),
                        ])];
                        content.extend(assistant_text_parts.iter().cloned());
                        assistant_message.insert("content".into(), JsValue::Array(content));
                    } else {
                        if !assistant_text.is_empty() {
                            assistant_message.insert("content".into(), JsValue::String(assistant_text));
                        }
                        let mut signature = non_empty_thinking
                            .first()
                            .and_then(|block| block.thinking_signature.clone());
                        if model.provider == "opencode-go" && signature.as_deref() == Some("reasoning") {
                            signature = Some("reasoning_content".into());
                        }
                        if let Some(signature) = signature.filter(|signature| !signature.is_empty()) {
                            assistant_message.insert(
                                signature.into(),
                                JsValue::String(pi_js_compat::js_json::JsString::join(
                                    non_empty_thinking.iter().map(|block| &block.thinking),
                                    "\n",
                                )),
                            );
                        }
                    }
                } else if !assistant_text.is_empty() {
                    assistant_message.insert("content".into(), JsValue::String(assistant_text));
                }

                let tool_calls = assistant
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        AssistantContent::ToolCall(tool_call) => Some(tool_call),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if !tool_calls.is_empty() {
                    let mut serialized_calls = Vec::new();
                    for tool_call in &tool_calls {
                        serialized_calls.push(lossless_object(vec![
                            ("id", JsValue::String(tool_call.id.clone().into())),
                            ("type", JsValue::String("function".into())),
                            (
                                "function",
                                lossless_object(vec![
                                    ("name", JsValue::String(tool_call.name.clone().into())),
                                    (
                                        "arguments",
                                        JsValue::String(
                                            pi_js_compat::js_json::stringify_lossless(
                                                &pi_js_compat::js_json::JsValue::Object(tool_call.arguments.clone()),
                                            )
                                            .into(),
                                        ),
                                    ),
                                ]),
                            ),
                        ]));
                    }
                    assistant_message.insert("tool_calls".into(), JsValue::Array(serialized_calls));

                    let reasoning_details = tool_calls
                        .iter()
                        .filter_map(|tool_call| tool_call.thought_signature.as_deref())
                        .filter_map(|signature| serde_json::from_str::<JsValue>(signature).ok())
                        .filter(pi_js_compat::js_json::is_truthy)
                        .collect::<Vec<_>>();
                    if !reasoning_details.is_empty() {
                        assistant_message.insert("reasoning_details".into(), JsValue::Array(reasoning_details));
                    }
                }
                if compat.requires_reasoning_content_on_assistant_messages
                    && model.reasoning
                    && !assistant_message.contains_key("reasoning_content")
                {
                    assistant_message.insert("reasoning_content".into(), JsValue::String(String::new().into()));
                }

                let has_content = match assistant_message.get("content") {
                    Some(JsValue::String(content)) => !content.is_empty(),
                    Some(JsValue::Array(content)) => !content.is_empty(),
                    _ => false,
                };
                if !has_content && !assistant_message.contains_key("tool_calls") {
                    index += 1;
                    continue;
                }
                params.push(JsValue::Object(assistant_message));
                last_role = Some("assistant".into());
            }
            Message::ToolResult(_) => {
                let mut image_blocks = Vec::new();
                let mut next = index;
                while next < transformed_messages.len() {
                    let Message::ToolResult(tool_result) = &transformed_messages[next] else {
                        break;
                    };
                    let text_result = tool_result
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            crate::types::TextOrImageContent::Text(text) => Some(sanitize_surrogates(&text.text)),
                            crate::types::TextOrImageContent::Image(_) => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let has_images = tool_result
                        .content
                        .iter()
                        .any(|block| matches!(block, crate::types::TextOrImageContent::Image(_)));
                    let has_text = !text_result.is_empty();
                    let tool_result_text = if has_text {
                        text_result
                    } else if has_images {
                        "(see attached image)".into()
                    } else {
                        "(no tool output)".into()
                    };
                    let mut tool_message = JsObject::new();
                    tool_message.insert("role".into(), JsValue::String("tool".into()));
                    tool_message.insert(
                        "content".into(),
                        JsValue::String(sanitize_surrogates(&tool_result_text).into()),
                    );
                    tool_message.insert(
                        "tool_call_id".into(),
                        JsValue::String(tool_result.tool_call_id.clone().into()),
                    );
                    if compat.requires_tool_result_name && !tool_result.tool_name.is_empty() {
                        tool_message.insert("name".into(), JsValue::String(tool_result.tool_name.clone().into()));
                    }
                    params.push(JsValue::Object(tool_message));

                    if has_images && model.input.contains(&crate::types::Modality::Image) {
                        for block in &tool_result.content {
                            if let crate::types::TextOrImageContent::Image(ImageContent { data, mime_type, .. }) = block
                            {
                                image_blocks.push(lossless_object(vec![
                                    ("type", JsValue::String("image_url".into())),
                                    (
                                        "image_url",
                                        lossless_object(vec![(
                                            "url",
                                            JsValue::String(format!("data:{mime_type};base64,{data}").into()),
                                        )]),
                                    ),
                                ]));
                            }
                        }
                    }
                    next += 1;
                }
                index = next.saturating_sub(1);

                if !image_blocks.is_empty() {
                    if compat.requires_assistant_after_tool_result {
                        params.push(lossless_object(vec![
                            ("role", JsValue::String("assistant".into())),
                            ("content", JsValue::String("I have processed the tool results.".into())),
                        ]));
                    }
                    let mut content = vec![lossless_object(vec![
                        ("type", JsValue::String("text".into())),
                        ("text", JsValue::String("Attached image(s) from tool result:".into())),
                    ])];
                    content.extend(image_blocks);
                    params.push(lossless_object(vec![
                        ("role", JsValue::String("user".into())),
                        ("content", JsValue::Array(content)),
                    ]));
                    last_role = Some("user".into());
                } else {
                    last_role = Some("toolResult".into());
                }

                index += 1;
                continue;
            }
        }
        index += 1;
    }
    params
}

fn convert_tools(tools: &[Tool], compat: &ResolvedOpenAICompletionsCompat) -> JsValue {
    let mut converted = Vec::new();
    for tool in tools {
        let mut function = JsObject::new();
        function.insert("name".into(), JsValue::String(tool.name.clone().into()));
        function.insert("description".into(), JsValue::String(tool.description.clone()));
        function.insert("parameters".into(), tool.parameters.clone().into());
        if compat.supports_strict_mode {
            function.insert("strict".into(), JsValue::Bool(false));
        }
        converted.push(lossless_object(vec![
            ("type", JsValue::String("function".into())),
            ("function", JsValue::Object(function)),
        ]));
    }
    JsValue::Array(converted)
}

// =============================================================================
// Cache control
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheControl {
    ttl: Option<String>,
}

impl CacheControl {
    fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("type".to_owned(), Value::String("ephemeral".to_owned()));
        if let Some(ttl) = &self.ttl {
            map.insert("ttl".to_owned(), Value::String(ttl.clone()));
        }
        Value::Object(map)
    }
}

fn get_compat_cache_control(
    compat: &ResolvedOpenAICompletionsCompat,
    cache_retention: CacheRetention,
) -> Option<CacheControl> {
    if compat.cache_control_format != Some(CacheControlFormat::Anthropic) || cache_retention == CacheRetention::None {
        return None;
    }
    let ttl =
        (cache_retention == CacheRetention::Long && compat.supports_long_cache_retention).then(|| "1h".to_owned());
    Some(CacheControl { ttl })
}

fn apply_anthropic_cache_control(messages: &mut [JsValue], tools: &mut Option<JsValue>, cache_control: &CacheControl) {
    for message in messages.iter_mut() {
        if message
            .get("role")
            .and_then(JsValue::as_string)
            .is_some_and(|role| role == "system" || role == "developer")
        {
            add_cache_control_to_text_content(message, cache_control);
            break;
        }
    }
    if let Some(JsValue::Array(tools)) = tools
        && let Some(last) = tools.last_mut()
        && let JsValue::Object(tool) = last
    {
        tool.insert("cache_control".into(), cache_control.to_value().into());
    }
    for message in messages.iter_mut().rev() {
        if message
            .get("role")
            .and_then(JsValue::as_string)
            .is_some_and(|role| role == "user" || role == "assistant" || role == "tool")
            && add_cache_control_to_text_content(message, cache_control)
        {
            break;
        }
    }
}

fn add_cache_control_to_text_content(message: &mut JsValue, cache_control: &CacheControl) -> bool {
    let JsValue::Object(message) = message else {
        return false;
    };
    match message.get_mut("content") {
        Some(JsValue::String(content)) if !content.is_empty() => {
            let text = std::mem::take(content);
            *message.get_mut("content").expect("content") = JsValue::Array(vec![lossless_object(vec![
                ("type", JsValue::String("text".into())),
                ("text", JsValue::String(text)),
                ("cache_control", cache_control.to_value().into()),
            ])]);
            true
        }
        Some(JsValue::Array(parts)) => {
            for part in parts.iter_mut().rev() {
                if part
                    .get("type")
                    .and_then(JsValue::as_string)
                    .is_some_and(|kind| kind == "text")
                    && let JsValue::Object(part) = part
                {
                    part.insert("cache_control".into(), cache_control.to_value().into());
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

// =============================================================================
// Params
// =============================================================================

fn thinking_level_from_str(value: &str) -> Option<ModelThinkingLevel> {
    match value {
        "off" => Some(ModelThinkingLevel::Off),
        "minimal" => Some(ModelThinkingLevel::Minimal),
        "low" => Some(ModelThinkingLevel::Low),
        "medium" => Some(ModelThinkingLevel::Medium),
        "high" => Some(ModelThinkingLevel::High),
        "xhigh" => Some(ModelThinkingLevel::Xhigh),
        "max" => Some(ModelThinkingLevel::Max),
        "ultra" => Some(ModelThinkingLevel::Ultra),
        _ => None,
    }
}

// Frozen607/729: lookup preserves missing, explicit null and native string units.
fn map_value(model: &Model, level: ModelThinkingLevel) -> Option<Option<JsString>> {
    model
        .thinking_level_map
        .as_ref()
        .and_then(|map| map.get(&level))
        .cloned()
}

/// `map[level] ?? fallback` for a requested effort string.
fn nullish_effort(model: &Model, effort: &str) -> JsString {
    // Frozen633/640/657/662/668 uses ??, not truthiness.
    thinking_level_from_str(effort)
        .and_then(|level| map_value(model, level))
        .flatten()
        .unwrap_or_else(|| effort.into())
}

fn insert_nullish_reasoning_effort(
    params: &mut JsObject,
    model: &Model,
    effort: Option<&str>,
    supports_reasoning_effort: bool,
) {
    if let Some(effort) = effort
        && supports_reasoning_effort
    {
        params.insert(
            "reasoning_effort".into(),
            JsValue::String(nullish_effort(model, effort)),
        );
    }
}

/// `map[level]` where a missing key falls back to the requested effort and an
/// explicit `null` stays `None`.
fn strict_effort(model: &Model, effort: &str) -> Option<JsString> {
    match thinking_level_from_str(effort).and_then(|level| map_value(model, level)) {
        None => Some(effort.into()),
        Some(value) => value,
    }
}

/// `map.off`; `None` means the key is absent, `Some(None)` means explicit null.
fn off_value(model: &Model) -> Option<Option<JsString>> {
    map_value(model, ModelThinkingLevel::Off)
}

fn resolve_chat_template_kwarg_value(model: &Model, effort: Option<&str>, value: &JsValue) -> Option<JsValue> {
    // Frozen717-731: primitive values pass through, every non-null object
    // follows the variable path (including unknown variable names and arrays).
    if !matches!(value, JsValue::Object(_) | JsValue::Array(_)) {
        return Some(value.clone());
    }
    if effort.is_none() && value.get("omitWhenOff").is_some_and(pi_js_compat::js_json::is_truthy) {
        return None;
    }
    if value
        .get("$var")
        .and_then(JsValue::as_string)
        .is_some_and(|value| value == "thinking.enabled")
    {
        return Some(JsValue::Bool(effort.is_some()));
    }
    match effort {
        Some(effort) => strict_effort(model, effort).map(JsValue::String),
        None => off_value(model).flatten().map(JsValue::String),
    }
}

fn build_chat_template_values(model: &Model, effort: Option<&str>, values: &JsValue) -> Option<JsValue> {
    // Frozen702: Object.entries enumerates own numeric keys before string keys.
    let properties = spread_compat_properties(values);
    let resolved = pi_js_compat::js_json::object_entries_lossless(&properties)
        .into_iter()
        .filter_map(|(key, value)| {
            resolve_chat_template_kwarg_value(model, effort, value).map(|value| (key.clone(), value))
        })
        .collect::<JsObject>();
    (!resolved.is_empty()).then_some(JsValue::Object(resolved))
}

pub fn build_params(
    model: &Model,
    context: &Context,
    stream_options: &StreamOptions,
    options: &OpenAICompletionsOptions,
    compat: &ResolvedOpenAICompletionsCompat,
    cache_retention: CacheRetention,
) -> JsValue {
    let messages = convert_messages(model, context, compat);
    let cache_control = get_compat_cache_control(compat, cache_retention);

    let mut params = JsObject::new();
    params.insert("model".into(), JsValue::String(model.id.clone().into()));
    params.insert("messages".into(), JsValue::Array(messages.clone()));
    params.insert("stream".into(), JsValue::Bool(true));
    if ((model.base_url.contains("api.openai.com") && cache_retention != CacheRetention::None)
        || (cache_retention == CacheRetention::Long && compat.supports_long_cache_retention))
        && let Some(key) = clamp_openai_prompt_cache_key(stream_options.session_id.as_ref())
    {
        params.insert("prompt_cache_key".into(), JsValue::String(key));
    }
    if cache_retention == CacheRetention::Long && compat.supports_long_cache_retention {
        params.insert("prompt_cache_retention".into(), JsValue::String("24h".into()));
    }
    if compat.supports_usage_in_streaming {
        params.insert(
            "stream_options".into(),
            lossless_object(vec![("include_usage", JsValue::Bool(true))]),
        );
    }
    if compat.supports_store {
        params.insert("store".into(), JsValue::Bool(false));
    }
    if let Some(max_tokens) = stream_options.max_tokens.filter(|max_tokens| *max_tokens != 0.0) {
        let field = match compat.max_tokens_field {
            MaxTokensField::MaxTokens => "max_tokens",
            MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
        };
        params.insert(field.into(), js_number_value(max_tokens).into());
    }
    if let Some(temperature) = stream_options.temperature {
        params.insert("temperature".into(), js_number_value(temperature).into());
    }

    let mut tools_value = None;
    if let Some(active_tools) = context.tools.as_deref().filter(|tools| !tools.is_empty()) {
        tools_value = Some(convert_tools(active_tools, compat));
        if compat.zai_tool_stream {
            params.insert("tool_stream".into(), JsValue::Bool(true));
        }
    } else if has_tool_history(&context.messages) {
        tools_value = Some(JsValue::Array(Vec::new()));
    }
    if let Some(tools) = &tools_value {
        params.insert("tools".into(), tools.clone());
    }

    if let Some(cache_control) = &cache_control {
        let mut cache_messages = messages;
        apply_anthropic_cache_control(&mut cache_messages, &mut tools_value, cache_control);
        params.insert("messages".into(), JsValue::Array(cache_messages));
        if let Some(tools) = tools_value {
            params.insert("tools".into(), tools);
        }
    }

    if let Some(tool_choice) = &options.tool_choice {
        params.insert("tool_choice".into(), tool_choice.clone().into());
    }

    let effort = options.reasoning_effort.as_deref();
    if compat.thinking_format == ThinkingFormat::Zai && model.reasoning {
        let thinking = if effort.is_some() {
            lossless_object(vec![
                ("type", JsValue::String("enabled".into())),
                ("clear_thinking", JsValue::Bool(false)),
            ])
        } else {
            lossless_object(vec![("type", JsValue::String("disabled".into()))])
        };
        params.insert("thinking".into(), thinking);
        if let Some(effort) = effort
            && compat.supports_reasoning_effort
            && let Some(mapped) = strict_effort(model, effort)
        {
            params.insert("reasoning_effort".into(), JsValue::String(mapped));
        }
    } else if compat.thinking_format == ThinkingFormat::Qwen && model.reasoning {
        params.insert("enable_thinking".into(), JsValue::Bool(effort.is_some()));
    } else if compat.thinking_format == ThinkingFormat::QwenChatTemplate && model.reasoning {
        params.insert(
            "chat_template_kwargs".into(),
            lossless_object(vec![
                ("enable_thinking", JsValue::Bool(effort.is_some())),
                ("preserve_thinking", JsValue::Bool(true)),
            ]),
        );
    } else if compat.thinking_format == ThinkingFormat::ChatTemplate && model.reasoning {
        if let Some(values) = build_chat_template_values(model, effort, &compat.chat_template_kwargs) {
            params.insert("chat_template_kwargs".into(), values);
        }
    } else if compat.thinking_format == ThinkingFormat::Deepseek && model.reasoning {
        if effort.is_some() {
            params.insert(
                "thinking".into(),
                lossless_object(vec![("type", JsValue::String("enabled".into()))]),
            );
        } else if off_value(model) != Some(None) {
            params.insert(
                "thinking".into(),
                lossless_object(vec![("type", JsValue::String("disabled".into()))]),
            );
        }
        insert_nullish_reasoning_effort(&mut params, model, effort, compat.supports_reasoning_effort);
    } else if compat.thinking_format == ThinkingFormat::Openrouter && model.reasoning {
        if let Some(effort) = effort {
            params.insert(
                "reasoning".into(),
                lossless_object(vec![("effort", JsValue::String(nullish_effort(model, effort)))]),
            );
        } else if off_value(model) != Some(None) {
            params.insert(
                "reasoning".into(),
                lossless_object(vec![(
                    "effort",
                    JsValue::String(off_value(model).flatten().unwrap_or_else(|| "none".into())),
                )]),
            );
        }
    } else if compat.thinking_format == ThinkingFormat::AntLing && model.reasoning && effort.is_some() {
        if let Some(mapped) = effort
            .and_then(thinking_level_from_str)
            .and_then(|level| map_value(model, level))
            .flatten()
        {
            params.insert(
                "reasoning".into(),
                lossless_object(vec![("effort", JsValue::String(mapped))]),
            );
        }
    } else if compat.thinking_format == ThinkingFormat::Together && model.reasoning {
        params.insert(
            "reasoning".into(),
            lossless_object(vec![("enabled", JsValue::Bool(effort.is_some()))]),
        );
        insert_nullish_reasoning_effort(&mut params, model, effort, compat.supports_reasoning_effort);
    } else if compat.thinking_format == ThinkingFormat::StringThinking && model.reasoning {
        if let Some(effort) = effort {
            params.insert("thinking".into(), JsValue::String(nullish_effort(model, effort)));
        } else if off_value(model) != Some(None) {
            params.insert(
                "thinking".into(),
                JsValue::String(off_value(model).flatten().unwrap_or_else(|| "none".into())),
            );
        }
    } else if let Some(effort) = effort
        && model.reasoning
        && compat.supports_reasoning_effort
    {
        params.insert(
            "reasoning_effort".into(),
            JsValue::String(nullish_effort(model, effort)),
        );
    } else if effort.is_none()
        && model.reasoning
        && compat.supports_reasoning_effort
        && let Some(off_value) = off_value(model).flatten()
    {
        params.insert("reasoning_effort".into(), JsValue::String(off_value));
    }

    if let Some(ModelCompat::OpenAICompletions(raw_compat)) = &model.compat {
        // Frozen677-690 reads the raw routing values using truthiness, not
        // typed arrays or Option presence. Never discard nested JSON or order.
        if let Some(routing) = raw_compat
            .open_router_routing
            .as_ref()
            .filter(|value| pi_js_compat::js_json::is_truthy(value))
        {
            params.insert("provider".into(), routing.clone());
        }
        if let Some(routing) = raw_compat
            .vercel_gateway_routing
            .as_ref()
            .filter(|value| pi_js_compat::js_json::is_truthy(value))
        {
            let gateway = ["only", "order"]
                .into_iter()
                .filter_map(|key| {
                    routing
                        .get(key)
                        .filter(|value| pi_js_compat::js_json::is_truthy(value))
                        .map(|value| (key.into(), value.clone()))
                })
                .collect::<JsObject>();
            if !gateway.is_empty() {
                params.insert(
                    "providerOptions".into(),
                    lossless_object(vec![("gateway", JsValue::Object(gateway))]),
                );
            }
        }
    }

    JsValue::Object(params)
}

// =============================================================================
// Client
// =============================================================================

fn merge_headers(
    model_headers: Option<&IndexMap<String, JsString>>,
    dynamic: Option<IndexMap<String, String>>,
) -> IndexMap<String, Option<JsString>> {
    let mut headers: IndexMap<String, Option<JsString>> = IndexMap::new();
    for (name, value) in model_headers.into_iter().flatten() {
        headers.insert(name.clone(), Some(value.clone()));
    }
    for (name, value) in dynamic.into_iter().flatten() {
        headers.insert(name, Some(value.into()));
    }
    headers
}

fn create_client(
    model: &Model,
    context: &Context,
    api_key: &JsString,
    stream_options: &StreamOptions,
    session_id: Option<&JsString>,
    compat: &ResolvedOpenAICompletionsCompat,
) -> SdkClient {
    let dynamic = (model.provider == "github-copilot").then(|| {
        let has_images = has_copilot_vision_input(&context.messages);
        build_copilot_dynamic_headers(&context.messages, has_images)
    });
    let mut headers = merge_headers(model.headers.as_ref(), dynamic);
    if let Some(session_id) = session_id
        && compat.send_session_affinity_headers
    {
        headers.insert("session_id".to_owned(), Some(session_id.clone()));
        headers.insert("x-client-request-id".to_owned(), Some(session_id.clone()));
        headers.insert("x-session-affinity".to_owned(), Some(session_id.clone()));
    }
    for (name, value) in stream_options.request.headers.iter().flatten() {
        headers.insert(name.clone(), value.clone());
    }
    SdkClient::new(SdkClientConfig {
        api_key: api_key.to_owned(),
        base_url: model.base_url.clone(),
        default_headers: headers,
        timeout_ms: stream_options.request.timeout_ms,
        fetch: stream_options.request.fetch.clone(),
        azure: None,
    })
}

// =============================================================================
// Stream processing
// =============================================================================

#[derive(Debug)]
pub enum CompletionsFailure {
    Sdk(SdkError),
    Request(Box<ProviderRequestError<CallbackError>>),
    Message(pi_js_compat::js_json::JsString),
}

impl CompletionsFailure {
    fn format(self) -> pi_js_compat::js_json::JsString {
        match self {
            Self::Sdk(error) => format_sdk_error(&error, None),
            Self::Request(error) => format_request_error(&error, None),
            Self::Message(message) => message,
        }
    }

    fn raw_metadata(&self) -> Option<&pi_js_compat::js_json::JsString> {
        let provider = match self {
            Self::Sdk(SdkError::Provider(error)) => Some(error.as_ref()),
            Self::Request(error) => match error.as_ref() {
                ProviderRequestError::Provider(error) => Some(error),
                _ => None,
            },
            _ => None,
        }?;
        provider
            .inner_error
            .as_deref()
            .and_then(|error| error.get("metadata"))
            .and_then(|metadata| metadata.get("raw"))
            .and_then(JsValue::as_string)
    }
}

fn finish_block(
    blocks: &[AssistantContent],
    block_index: usize,
    sender: &AssistantMessageEventSender,
    output: &AssistantMessage,
) {
    match &blocks[block_index] {
        AssistantContent::Text(text) => sender.push(AssistantMessageEvent::TextEnd {
            content_index: block_index,
            content: text.text.clone(),
            partial: output.clone(),
        }),
        AssistantContent::Thinking(thinking) => sender.push(AssistantMessageEvent::ThinkingEnd {
            content_index: block_index,
            content: thinking.thinking.clone(),
            partial: output.clone(),
        }),
        AssistantContent::ToolCall(tool_call) => sender.push(AssistantMessageEvent::ToolCallEnd {
            content_index: block_index,
            tool_call: Box::new(tool_call.clone()),
            partial: output.clone(),
        }),
    }
}

fn parse_arguments(partial_args: Option<&pi_js_compat::js_json::JsString>) -> JsObject {
    match parse_streaming_json(partial_args) {
        JsValue::Object(map) => map,
        _ => JsObject::new(),
    }
}

fn apply_pending_reasoning_detail(tool_call: &mut ToolCall, pending: &mut BTreeMap<String, String>) {
    if tool_call.id.is_empty() {
        return;
    }
    if let Some(detail) = pending.remove(&tool_call.id) {
        tool_call.thought_signature = Some(detail);
    }
}

fn is_encrypted_ingress_detail(detail: &JsValue) -> bool {
    detail.get("type").and_then(ingress_metadata_text).as_deref() == Some("reasoning.encrypted")
        && detail
            .get("id")
            .and_then(JsValue::as_string)
            .is_some_and(|id| !id.is_empty())
        && detail
            .get("data")
            .and_then(JsValue::as_string)
            .is_some_and(|data| !data.is_empty())
}

fn stream_index_key(index: f64) -> u64 {
    if index == 0.0 { 0 } else { index.to_bits() }
}

fn ensure_tool_call_block(
    blocks: &mut Vec<AssistantContent>,
    by_index: &mut BTreeMap<u64, usize>,
    by_id: &mut BTreeMap<String, usize>,
    tool_call: &JsValue,
    pending: &mut BTreeMap<String, String>,
) -> (usize, bool) {
    let stream_index = tool_call.get("index").and_then(JsValue::as_f64);
    let name = tool_call
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(ingress_metadata_text)
        .unwrap_or_default();
    let id = tool_call.get("id").and_then(ingress_metadata_text).unwrap_or_default();

    let mut block_index = stream_index.and_then(|index| by_index.get(&stream_index_key(index)).copied());
    if block_index.is_none() && !id.is_empty() {
        block_index = by_id.get(&id).copied();
    }
    let mut created = false;
    if block_index.is_none() {
        let mut block = ToolCall::new(id.to_owned(), &name, JsObject::new());
        block.partial_args = Some(Default::default());
        block.stream_index = stream_index;
        blocks.push(AssistantContent::ToolCall(block));
        let index = blocks.len() - 1;
        if let Some(stream_index) = stream_index {
            by_index.insert(stream_index_key(stream_index), index);
        }
        if !id.is_empty() {
            by_id.insert(id.to_owned(), index);
        }
        created = true;
        block_index = Some(index);
    }
    let index = block_index.expect("tool call block");
    let AssistantContent::ToolCall(tool_call_block) = &mut blocks[index] else {
        unreachable!("tool call index");
    };
    if let Some(stream_index) = stream_index
        && tool_call_block.stream_index.is_none()
    {
        tool_call_block.stream_index = Some(stream_index);
        by_index.insert(stream_index_key(stream_index), index);
    }
    if !id.is_empty() {
        by_id.insert(id.to_owned(), index);
    }
    if tool_call_block.name.is_empty() && !name.is_empty() {
        tool_call_block.name = name.to_owned();
    }
    apply_pending_reasoning_detail(tool_call_block, pending);
    (index, created)
}

pub async fn process_completions_stream(
    stream: &mut crate::api::openai_sdk::SseJsonStream,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    model: &Model,
) -> Result<bool, CompletionsFailure> {
    let mut text_block: Option<usize> = None;
    let mut thinking_block: Option<usize> = None;
    let mut has_finish_reason = false;
    let mut tool_by_index: BTreeMap<u64, usize> = BTreeMap::new();
    let mut tool_by_id: BTreeMap<String, usize> = BTreeMap::new();
    let mut pending_reasoning_details: BTreeMap<String, String> = BTreeMap::new();

    loop {
        let chunk = match stream.next_event().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => return Err(CompletionsFailure::Sdk(error)),
        };
        if !chunk.is_object() {
            continue;
        }
        crate::api::openai_sdk::validate_chat_metadata(&chunk)
            .map_err(|error| CompletionsFailure::Message(error.into()))?;

        if output.response_id.as_deref().is_none_or(str::is_empty)
            && let Some(id) = chunk.get("id").and_then(ingress_metadata_text).as_deref()
        {
            output.response_id = Some(id.to_owned());
        }
        if let Some(chunk_model) = chunk.get("model").and_then(ingress_metadata_text).as_deref()
            && !chunk_model.is_empty()
            && chunk_model != model.id
            && output.response_model.as_deref().is_none_or(str::is_empty)
        {
            output.response_model = Some(chunk_model.to_owned());
        }
        let chunk_usage = chunk.get("usage").filter(|usage| usage.is_object());
        if let Some(usage) = chunk_usage {
            output.usage = parse_chunk_usage(usage, model);
        }

        let Some(choice) = chunk
            .get("choices")
            .and_then(JsValue::as_array)
            .and_then(|choices| choices.first())
        else {
            continue;
        };
        if chunk_usage.is_none()
            && let Some(usage) = choice.get("usage").filter(|usage| usage.is_object())
        {
            output.usage = parse_chunk_usage(usage, model);
        }

        if let Some(finish_reason) = choice.get("finish_reason").and_then(ingress_metadata_text).as_deref()
            && !finish_reason.is_empty()
        {
            let mapped = map_stop_reason(finish_reason);
            output.stop_reason = mapped.stop_reason;
            if mapped.error_message.is_some() {
                output.error_message = mapped.error_message;
            }
            has_finish_reason = true;
        }

        let Some(delta) = choice.get("delta").filter(|delta| delta.is_object()) else {
            continue;
        };

        if let Some(content) = delta.get("content").and_then(JsValue::as_string)
            && !content.is_empty()
        {
            let index = match text_block {
                Some(index) => index,
                None => {
                    output.content.push(AssistantContent::Text(TextContent::new("")));
                    let index = output.content.len() - 1;
                    text_block = Some(index);
                    sender.push(AssistantMessageEvent::TextStart {
                        content_index: index,
                        partial: output.clone(),
                    });
                    index
                }
            };
            if let AssistantContent::Text(block) = &mut output.content[index] {
                block.text.push_str(content);
            }
            sender.push(AssistantMessageEvent::TextDelta {
                content_index: index,
                delta: content.into(),
                partial: output.clone(),
            });
        }

        let reasoning_fields = ["reasoning_content", "reasoning", "reasoning_text"];
        let found_reasoning = reasoning_fields
            .iter()
            .find(|field| {
                delta
                    .get(field)
                    .and_then(JsValue::as_string)
                    .is_some_and(|value| !value.is_empty())
            })
            .copied();
        if let Some(field) = found_reasoning {
            let delta_text = delta
                .get(field)
                .and_then(JsValue::as_string)
                .cloned()
                .unwrap_or_default();
            let signature = if model.provider == "opencode-go" && field == "reasoning" {
                "reasoning_content"
            } else {
                field
            };
            let index = match thinking_block {
                Some(index) => index,
                None => {
                    output.content.push(AssistantContent::Thinking(ThinkingContent {
                        r#type: crate::types::ThinkingTag::Tag,
                        thinking: String::new().into(),
                        thinking_signature: Some(signature.to_owned()),
                        redacted: None,
                        index: None,
                    }));
                    let index = output.content.len() - 1;
                    thinking_block = Some(index);
                    sender.push(AssistantMessageEvent::ThinkingStart {
                        content_index: index,
                        partial: output.clone(),
                    });
                    index
                }
            };
            if let AssistantContent::Thinking(block) = &mut output.content[index] {
                block.thinking.push_str(&delta_text);
            }
            sender.push(AssistantMessageEvent::ThinkingDelta {
                content_index: index,
                delta: delta_text.to_owned(),
                partial: output.clone(),
            });
        }

        if let Some(tool_calls) = delta.get("tool_calls").and_then(JsValue::as_array) {
            for tool_call in tool_calls {
                let (index, created) = ensure_tool_call_block(
                    &mut output.content,
                    &mut tool_by_index,
                    &mut tool_by_id,
                    tool_call,
                    &mut pending_reasoning_details,
                );
                if created {
                    sender.push(AssistantMessageEvent::ToolCallStart {
                        content_index: index,
                        partial: output.clone(),
                    });
                }
                if let Some(id) = tool_call.get("id").and_then(ingress_metadata_text).as_deref()
                    && let AssistantContent::ToolCall(block) = &mut output.content[index]
                    && block.id.is_empty()
                {
                    block.id = id.to_owned();
                    tool_by_id.insert(id.to_owned(), index);
                }
                if let Some(name) = tool_call
                    .get("function")
                    .and_then(|function| function.get("name"))
                    .and_then(ingress_metadata_text)
                    .as_deref()
                    && let AssistantContent::ToolCall(block) = &mut output.content[index]
                    && block.name.is_empty()
                {
                    block.name = name.to_owned();
                }

                let mut delta_text = pi_js_compat::js_json::JsString::default();
                if let Some(arguments) = tool_call
                    .get("function")
                    .and_then(|function| function.get("arguments"))
                    .and_then(JsValue::as_string)
                    .filter(|arguments| !arguments.is_empty())
                {
                    delta_text = arguments.to_owned();
                    if let AssistantContent::ToolCall(block) = &mut output.content[index] {
                        let partial = block.partial_args.get_or_insert_default();
                        partial.push_str(arguments);
                        block.arguments = parse_arguments(Some(partial));
                    }
                }
                sender.push(AssistantMessageEvent::ToolCallDelta {
                    content_index: index,
                    delta: delta_text,
                    partial: output.clone(),
                });
            }
        }

        if let Some(reasoning_details) = delta.get("reasoning_details").and_then(JsValue::as_array) {
            for detail in reasoning_details {
                if is_encrypted_ingress_detail(detail) {
                    let serialized = pi_js_compat::js_json::stringify_lossless(detail);
                    let id = detail.get("id").and_then(ingress_metadata_text).unwrap_or_default();
                    if let Some(index) = tool_by_id.get(&id).copied() {
                        if let AssistantContent::ToolCall(block) = &mut output.content[index] {
                            block.thought_signature = Some(serialized);
                        }
                    } else {
                        pending_reasoning_details.insert(id.to_owned(), serialized);
                    }
                }
            }
        }
    }

    for index in 0..output.content.len() {
        if let AssistantContent::ToolCall(block) = &mut output.content[index] {
            block.arguments = parse_arguments(block.partial_args.as_ref());
            block.partial_args = None;
            block.stream_index = None;
        }
        finish_block(&output.content, index, sender, output);
    }
    Ok(has_finish_reason)
}

fn parse_chunk_usage(usage: &JsValue, model: &Model) -> Usage {
    let prompt_tokens = usage.get("prompt_tokens").and_then(JsValue::as_f64).unwrap_or(0.0);
    let prompt_details = usage.get("prompt_tokens_details");
    let cache_read_tokens = prompt_details
        .and_then(|details| details.get("cached_tokens"))
        .and_then(JsValue::as_f64)
        .or_else(|| usage.get("prompt_cache_hit_tokens").and_then(JsValue::as_f64))
        .unwrap_or(0.0);
    let cache_write_tokens = prompt_details
        .and_then(|details| details.get("cache_write_tokens"))
        .and_then(JsValue::as_f64)
        .unwrap_or(0.0);
    let input = (prompt_tokens - cache_read_tokens - cache_write_tokens).max(0.0);
    let output_tokens = usage.get("completion_tokens").and_then(JsValue::as_f64).unwrap_or(0.0);
    let mut parsed = Usage {
        input,
        output: output_tokens,
        cache_read: cache_read_tokens,
        cache_write: cache_write_tokens,
        cache_write_1h: None,
        reasoning: usage
            .get("completion_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(JsValue::as_f64)
            .unwrap_or(0.0)
            .into(),
        total_tokens: input + output_tokens + cache_read_tokens + cache_write_tokens,
        cost: UsageCost::default(),
    };
    parsed.cost = calculate_cost(model, &parsed);
    parsed
}

struct MappedStopReason {
    stop_reason: StopReason,
    error_message: Option<pi_js_compat::js_json::JsString>,
}

fn map_stop_reason(reason: &str) -> MappedStopReason {
    match reason {
        "stop" | "end" => MappedStopReason {
            stop_reason: StopReason::Stop,
            error_message: None,
        },
        "length" => MappedStopReason {
            stop_reason: StopReason::Length,
            error_message: None,
        },
        "function_call" | "tool_calls" => MappedStopReason {
            stop_reason: StopReason::ToolUse,
            error_message: None,
        },
        "content_filter" => MappedStopReason {
            stop_reason: StopReason::Error,
            error_message: Some("Provider finish_reason: content_filter".to_owned().into()),
        },
        "network_error" => MappedStopReason {
            stop_reason: StopReason::Error,
            error_message: Some("Provider finish_reason: network_error".to_owned().into()),
        },
        other => MappedStopReason {
            stop_reason: StopReason::Error,
            error_message: Some(format!("Provider finish_reason: {other}").into()),
        },
    }
}

fn done_reason(stop_reason: StopReason) -> Option<DoneReason> {
    match stop_reason {
        StopReason::Stop => Some(DoneReason::Stop),
        StopReason::Length => Some(DoneReason::Length),
        StopReason::ToolUse => Some(DoneReason::ToolUse),
        StopReason::Error | StopReason::Aborted => None,
    }
}

// =============================================================================
// Stream entry points
// =============================================================================

fn retry_options(options: &ProviderRequestOptions) -> ProviderRetryOptions {
    ProviderRetryOptions {
        max_retries: options
            .max_retries
            .filter(|retries| *retries > 0.0)
            .map(|retries| retries as usize)
            .unwrap_or(0),
        max_retry_delay_ms: options.max_retry_delay_ms,
        signal: options.signal.clone(),
    }
}

async fn run_stream(
    model: &Model,
    context: &Context,
    stream_options: &StreamOptions,
    options: &OpenAICompletionsOptions,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
) -> Result<(), CompletionsFailure> {
    let api_key = get_client_api_key(
        &model.provider,
        stream_options.request.api_key.as_ref(),
        stream_options.request.headers.as_ref(),
    )
    .map_err(|error| CompletionsFailure::Message(error.into()))?;
    let compat = get_compat(model);
    let cache_retention = resolve_cache_retention(stream_options.cache_retention, stream_options.request.env.as_ref());
    let cache_session_id = (cache_retention != CacheRetention::None)
        .then_some(stream_options.session_id.as_ref())
        .flatten();
    let client = create_client(model, context, &api_key, stream_options, cache_session_id, &compat);
    let params = build_params(model, context, stream_options, options, &compat, cache_retention);

    let mut params = params;
    if let Some(on_payload) = &stream_options.request.on_payload
        && let Some(next_params) = on_payload(params.clone(), model.clone())
            .await
            .map_err(|error| CompletionsFailure::Message(error.message))?
    {
        params = next_params;
    }

    let signal = stream_options.request.signal.clone();
    let mut retry_count = 0;
    let response = retry_provider_request(
        || {
            let client = client.for_retry(retry_count);
            retry_count += 1;
            let params = params.clone();
            let signal = signal.clone();
            async move { client.create_chat_completion_stream(params, signal).await }
        },
        retry_options(&stream_options.request),
    )
    .await
    .map_err(|error| CompletionsFailure::Request(Box::new(error)))?;

    if let Some(on_response) = &stream_options.request.on_response {
        on_response(
            ProviderResponse {
                status: f64::from(response.status),
                headers: headers_to_record(response.headers.clone()),
            },
            model.clone(),
        )
        .await
        .map_err(|error| CompletionsFailure::Message(error.message))?;
    }
    sender.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });

    let mut stream = response.stream;
    let has_finish_reason = process_completions_stream(&mut stream, output, sender, model).await?;

    if stream_options.request.signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Err(CompletionsFailure::Message("Request was aborted".to_owned().into()));
    }
    if output.stop_reason == StopReason::Aborted {
        return Err(CompletionsFailure::Message("Request was aborted".to_owned().into()));
    }
    if output.stop_reason == StopReason::Error {
        return Err(CompletionsFailure::Message(
            output
                .error_message
                .clone()
                .unwrap_or_else(|| "Provider returned an error stop reason".to_owned().into()),
        ));
    }
    // Frozen453: finish_reason is required regardless of unknown compat keys.
    if !has_finish_reason {
        return Err(CompletionsFailure::Message(
            "Stream ended without finish_reason".to_owned().into(),
        ));
    }
    let Some(reason) = done_reason(output.stop_reason) else {
        return Err(CompletionsFailure::Message(
            "Stream ended without finish_reason".to_owned().into(),
        ));
    };
    sender.push(AssistantMessageEvent::Done {
        reason,
        message: output.clone(),
    });
    Ok(())
}

fn run_and_emit(
    model: &Model,
    context: &Context,
    stream_options: StreamOptions,
) -> crate::utils::event_stream::AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    let model = model.clone();
    let context = context.clone();
    tokio::spawn(async move {
        let options = OpenAICompletionsOptions::from_stream_options(&stream_options);
        let mut output = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis() as f64);
        let result = run_stream(&model, &context, &stream_options, &options, &mut output, &sender).await;
        if let Err(failure) = result {
            for block in &mut output.content {
                match block {
                    AssistantContent::Text(block) => block.index = None,
                    AssistantContent::Thinking(block) => block.index = None,
                    AssistantContent::ToolCall(block) => {
                        block.index = None;
                        block.partial_args = None;
                        block.stream_index = None;
                    }
                }
            }
            output.stop_reason = if stream_options.request.signal.as_ref().is_some_and(AbortSignal::aborted) {
                StopReason::Aborted
            } else {
                StopReason::Error
            };
            let raw_metadata = failure.raw_metadata().cloned();
            let mut message = failure.format();
            if let Some(raw) = raw_metadata
                && !message.contains(&raw)
            {
                message.push_str("\n");
                message.push_str(&raw);
            }
            output.error_message = Some(message);
            sender.push(AssistantMessageEvent::Error {
                reason: if output.stop_reason == StopReason::Aborted {
                    ErrorReason::Aborted
                } else {
                    ErrorReason::Error
                },
                error: output.clone(),
            });
            sender.end(Some(output));
        }
    });
    stream
}

pub fn stream(
    model: Model,
    context: Context,
    options: StreamOptions,
) -> crate::utils::event_stream::AssistantMessageEventStream {
    run_and_emit(&model, &context, options)
}

pub fn stream_simple(
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> crate::utils::event_stream::AssistantMessageEventStream {
    let api_key = match get_client_api_key(
        &model.provider,
        options.stream.request.api_key.as_ref(),
        options.stream.request.headers.as_ref(),
    ) {
        Ok(api_key) => api_key,
        Err(message) => return crate::request::error_stream(&model, &message),
    };
    let base = build_base_options(&model, &context, Some(&options), Some(&api_key));
    let clamped_reasoning = options
        .reasoning
        .map(|reasoning| clamp_thinking_level(&model, model_thinking_level(reasoning)));
    let reasoning_effort = clamp_standard_reasoning_level(clamped_reasoning);
    let mut stream_options = base;
    if let Some(reasoning_effort) = reasoning_effort {
        stream_options
            .extra
            .insert("reasoningEffort".to_owned(), Value::String(reasoning_effort));
    }
    if let Some(thinking_budgets) = options.thinking_budgets {
        stream_options.extra.insert(
            "thinkingBudgets".to_owned(),
            serde_json::to_value(thinking_budgets).unwrap_or(Value::Null),
        );
    }
    run_and_emit(&model, &context, stream_options)
}

/// `ProviderStreams` adapter for `openAICompletionsApi()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenAICompletionsApi;

impl ProviderStreams for OpenAICompletionsApi {
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<crate::utils::event_stream::AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(stream(model, context, options))
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<crate::utils::event_stream::AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(stream_simple(model, context, options))
    }
}

/// `openAICompletionsApi()` from `openai-completions.lazy.ts`.
pub fn open_ai_completions_api() -> LazyApi {
    lazy_api(Arc::new(|| {
        Box::pin(async { Ok(Arc::new(OpenAICompletionsApi) as Arc<dyn ProviderStreams>) })
            as BoxFuture<'static, Result<Arc<dyn ProviderStreams>, CallbackError>>
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(provider: &str, id: &str) -> Model {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": id, "api": "openai-completions", "provider": provider,
            "baseUrl": "https://api.openai.com/v1", "reasoning": true,
            "input": ["text"],
            "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0},
            "contextWindow": 128000, "maxTokens": 4096
        }))
        .expect("model")
    }

    #[test]
    fn tool_call_ids_preserve_item_uniqueness() {
        let model = model("openai", "gpt-5");
        assert_eq!(normalize_tool_call_id("call_1|item_1", &model), "call_1_item_1");
        assert_eq!(normalize_tool_call_id("short", &model), "short");
        let long = format!("call_{}|item_{}", "x".repeat(40), "y".repeat(40));
        let normalized = normalize_tool_call_id(&long, &model);
        assert!(normalized.len() <= 40);
    }

    #[test]
    fn compat_detection_matches_provider_rules() {
        let openai = get_compat(&model("openai", "gpt-5"));
        assert!(openai.supports_store);
        assert!(openai.supports_developer_role);
        assert_eq!(openai.max_tokens_field, MaxTokensField::MaxCompletionTokens);

        let deepseek = get_compat(&model("deepseek", "deepseek-chat"));
        assert!(!deepseek.supports_store);
        assert_eq!(deepseek.max_tokens_field, MaxTokensField::MaxCompletionTokens);
        assert_eq!(deepseek.thinking_format, ThinkingFormat::Deepseek);
        assert!(deepseek.requires_reasoning_content_on_assistant_messages);
    }

    #[test]
    fn stop_reason_mapping_matches_provider_names() {
        assert_eq!(map_stop_reason("stop").stop_reason, StopReason::Stop);
        assert_eq!(map_stop_reason("end").stop_reason, StopReason::Stop);
        assert_eq!(map_stop_reason("tool_calls").stop_reason, StopReason::ToolUse);
        assert_eq!(map_stop_reason("length").stop_reason, StopReason::Length);
        assert_eq!(
            map_stop_reason("content_filter").error_message.as_ref(),
            Some(&"Provider finish_reason: content_filter".into())
        );
    }
}
