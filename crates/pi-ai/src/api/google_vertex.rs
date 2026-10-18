//! Port of `packages/ai/src/api/google-vertex.ts`.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::api::google_client::{GoogleClient, GoogleStream};
use crate::api::google_generative_ai::GoogleThinkingOptions;
use crate::api::google_shared::{
    GoogleRequestError, GoogleRetryOptions, GoogleThinkingLevel, convert_messages, convert_tools, is_thinking_part,
    map_stop_reason, resolve_google_function_calling_mode, retain_thought_signature, retry_google_request,
    supports_google_strict_tool_sampling,
};
use crate::api::lazy::{LazyApi, lazy_api};
use crate::api::simple_options::build_base_options;
use crate::http::default_fetch;
use crate::models::{calculate_cost, clamp_thinking_level};
use crate::request::{ProviderStreams, SimpleStreamOptions, StreamOptions, error_stream, now_millis};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, ErrorReason, Model,
    ModelThinkingLevel, ProviderEnv, StopReason, TextContent, ThinkingBudgets, ThinkingContent, ToolCall, Usage,
    UsageCost,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::sanitize_unicode::sanitize_surrogates;

const GCP_VERTEX_CREDENTIALS_MARKER: &str = "gcp-vertex-credentials";

static TOOL_CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

/// `GoogleVertexOptions` from the TypeScript adapter.
#[derive(Clone, Default)]
pub struct GoogleVertexOptions {
    pub stream: StreamOptions,
    pub tool_choice: Option<String>,
    pub thinking: Option<GoogleThinkingOptions>,
    pub project: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleVertexOptionsExtra {
    #[serde(default)]
    tool_choice: Option<String>,
    #[serde(default)]
    thinking: Option<GoogleThinkingOptions>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    location: Option<String>,
}

impl GoogleVertexOptions {
    /// Parse the API-specific fields carried by [`StreamOptions::extra`].
    pub fn from_stream_options(stream: StreamOptions) -> Result<Self, String> {
        let extra: GoogleVertexOptionsExtra =
            serde_json::from_value(Value::Object(stream.extra.clone())).map_err(|error| error.to_string())?;
        Ok(Self {
            stream,
            tool_choice: extra.tool_choice,
            thinking: extra.thinking,
            project: extra.project,
            location: extra.location,
        })
    }
}

/// `stream()`.
pub fn stream(model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
    let options = match GoogleVertexOptions::from_stream_options(options) {
        Ok(options) => options,
        Err(message) => return error_stream(&model, &message),
    };
    run_stream(model, context, options)
}

/// `streamSimple()`.
pub fn stream_simple(model: Model, context: Context, options: SimpleStreamOptions) -> AssistantMessageEventStream {
    let base = build_base_options(&model, &context, Some(&options), None);
    let thinking = match options.reasoning {
        None => Some(GoogleThinkingOptions {
            enabled: false,
            budget_tokens: None,
            level: None,
        }),
        Some(reasoning) => {
            let clamped = clamp_thinking_level(&model, model_thinking_level(reasoning));
            let effort = if clamped == ModelThinkingLevel::Off {
                ModelThinkingLevel::High
            } else {
                clamped
            };
            if is_gemini3_pro_model(&model) || is_gemini3_flash_model(&model) {
                Some(GoogleThinkingOptions {
                    enabled: true,
                    budget_tokens: None,
                    level: get_gemini3_thinking_level(effort, &model),
                })
            } else {
                Some(GoogleThinkingOptions {
                    enabled: true,
                    budget_tokens: get_google_budget(&model, effort, options.thinking_budgets.as_ref()),
                    level: None,
                })
            }
        }
    };
    // TypeScript's `buildBaseOptions` does not copy `project`/`location`, so
    // `streamSimple` only reaches Vertex through env-provided project/location.
    run_stream(
        model,
        context,
        GoogleVertexOptions {
            stream: base,
            tool_choice: None,
            thinking,
            project: None,
            location: None,
        },
    )
}

/// `ProviderStreams` adapter for `googleVertexApi()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct GoogleVertexApi;

impl ProviderStreams for GoogleVertexApi {
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

/// `googleVertexApi()` from `google-vertex.lazy.ts`.
pub fn google_vertex_api() -> LazyApi {
    lazy_api(
        std::sync::Arc::new(|| {
            Box::pin(async { Ok(std::sync::Arc::new(GoogleVertexApi) as std::sync::Arc<dyn ProviderStreams>) })
        }),
        Default::default(),
    )
}

fn model_thinking_level(level: crate::types::ThinkingLevel) -> ModelThinkingLevel {
    match level {
        crate::types::ThinkingLevel::Minimal => ModelThinkingLevel::Minimal,
        crate::types::ThinkingLevel::Low => ModelThinkingLevel::Low,
        crate::types::ThinkingLevel::Medium => ModelThinkingLevel::Medium,
        crate::types::ThinkingLevel::High => ModelThinkingLevel::High,
        crate::types::ThinkingLevel::Xhigh => ModelThinkingLevel::Xhigh,
        crate::types::ThinkingLevel::Max => ModelThinkingLevel::Max,
        crate::types::ThinkingLevel::Ultra => ModelThinkingLevel::Ultra,
    }
}

fn run_stream(model: Model, context: Context, options: GoogleVertexOptions) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    let signal = options.stream.request.signal.clone();
    tokio::spawn(async move {
        let mut output =
            AssistantMessage::empty("google-vertex", model.provider.clone(), model.id.clone(), now_millis());
        match run(&sender, &mut output, &model, &context, &options, signal.as_ref()).await {
            Ok(()) => {}
            Err(message) => {
                let reason = if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                    output.stop_reason = StopReason::Aborted;
                    ErrorReason::Aborted
                } else {
                    output.stop_reason = StopReason::Error;
                    ErrorReason::Error
                };
                output.error_message = Some(message);
                sender.push(AssistantMessageEvent::Error {
                    reason,
                    error: output.clone(),
                });
                sender.end(Some(output));
            }
        }
    });
    stream
}

async fn run(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    model: &Model,
    context: &Context,
    options: &GoogleVertexOptions,
    signal: Option<&CancellationToken>,
) -> Result<(), String> {
    let fetch = options.stream.request.fetch.clone().unwrap_or_else(default_fetch);
    let client = match resolve_api_key(options) {
        Some(api_key) => GoogleClient::vertex_api_key(model, &api_key, options.stream.request.headers.as_ref()),
        None => {
            let project = resolve_project(options)?;
            let location = resolve_location(options)?;
            GoogleClient::vertex_adc(
                model,
                &project,
                &location,
                options.stream.request.headers.as_ref(),
                options.stream.request.env.as_ref(),
            )
            .map_err(|error| error.message)?
        }
    };
    let params = Value::Object(build_params(model, context, options).map_err(|error| error.message)?);
    let params = match &options.stream.request.on_payload {
        Some(on_payload) => on_payload(params.clone(), model.clone())
            .await
            .map_err(|error| error.message)?
            .unwrap_or(params),
        None => params,
    };
    let retry_options = GoogleRetryOptions {
        max_retries: options.stream.request.max_retries,
        max_retry_delay_ms: options.stream.request.max_retry_delay_ms,
        signal: signal.cloned(),
    };
    let mut google_stream = retry_google_request(
        || client.generate_content_stream(&params, fetch.as_ref(), signal),
        retry_options,
    )
    .await
    .map_err(|error| error.message)?;

    sender.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    pump_stream(&mut google_stream, output, sender, model)
        .await
        .map_err(|error| error.message)?;

    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err("Request was aborted".to_owned());
    }
    if output.stop_reason == StopReason::Pending {
        return Err("Google Vertex stream ended without a finish reason".to_owned());
    }
    if matches!(output.stop_reason, StopReason::Aborted | StopReason::Error) {
        return Err(match &output.raw_stop_reason {
            Some(reason) => format!("Provider stopped with: {reason}"),
            None => "An unknown error occurred".to_owned(),
        });
    }
    let reason = match output.stop_reason {
        StopReason::Stop => DoneReason::Stop,
        StopReason::Length => DoneReason::Length,
        StopReason::ToolUse => DoneReason::ToolUse,
        StopReason::Deferred => DoneReason::Deferred,
        StopReason::Pending | StopReason::Error | StopReason::Aborted => {
            return Err("An unknown error occurred".to_owned());
        }
    };
    sender.push(AssistantMessageEvent::Done {
        reason,
        message: output.clone(),
    });
    sender.end(None);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Text,
    Thinking,
}

async fn pump_stream(
    google_stream: &mut GoogleStream,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    model: &Model,
) -> Result<(), GoogleRequestError> {
    let mut current: Option<(BlockKind, usize)> = None;
    while let Some(chunk) = google_stream.next_chunk().await {
        let chunk = chunk?;
        if output.response_id.as_deref().unwrap_or_default().is_empty()
            && let Some(response_id) = chunk.response_id.clone()
        {
            output.response_id = Some(response_id);
        }
        if let Some(candidate) = chunk.candidates.as_ref().and_then(|candidates| candidates.first()) {
            if let Some(parts) = candidate.content.as_ref().and_then(|content| content.parts.as_ref()) {
                for part in parts {
                    if let Some(text) = &part.text {
                        let thinking = is_thinking_part(part.thought);
                        let needs_block = match current {
                            None => true,
                            Some((BlockKind::Thinking, _)) => !thinking,
                            Some((BlockKind::Text, _)) => thinking,
                        };
                        if needs_block {
                            close_block(sender, output, current);
                            current = Some(open_block(sender, output, thinking));
                        }
                        let (kind, index) = current.expect("a block was just opened");
                        append_text(output, kind, index, text, part.thought_signature.as_deref());
                        let event = match kind {
                            BlockKind::Text => AssistantMessageEvent::TextDelta {
                                content_index: index,
                                delta: text.clone(),
                                partial: output.clone(),
                            },
                            BlockKind::Thinking => AssistantMessageEvent::ThinkingDelta {
                                content_index: index,
                                delta: text.clone(),
                                partial: output.clone(),
                            },
                        };
                        sender.push(event);
                    }
                    if let Some(function_call) = &part.function_call {
                        if current.is_some() {
                            close_block(sender, output, current);
                            current = None;
                        }
                        let provided_id = function_call.id.clone();
                        let duplicate = provided_id.as_ref().is_some_and(|id| {
                            output
                                .content
                                .iter()
                                .any(|block| matches!(block, AssistantContent::ToolCall(call) if &call.id == id))
                        });
                        let tool_call_id = match &provided_id {
                            Some(id) if !duplicate => id.clone(),
                            _ => generated_tool_call_id(function_call.name.as_deref().unwrap_or("")),
                        };
                        let mut arguments = Map::new();
                        if let Some(Value::Object(args)) = &function_call.args {
                            arguments = args.clone();
                        }
                        let mut tool_call =
                            ToolCall::new(tool_call_id, function_call.name.clone().unwrap_or_default(), arguments);
                        tool_call.thought_signature =
                            part.thought_signature.clone().filter(|signature| !signature.is_empty());
                        let index = output.content.len();
                        output.content.push(AssistantContent::ToolCall(tool_call.clone()));
                        sender.push(AssistantMessageEvent::ToolCallStart {
                            content_index: index,
                            partial: output.clone(),
                        });
                        sender.push(AssistantMessageEvent::ToolCallDelta {
                            content_index: index,
                            delta: crate::api::google_client::js_json(&Value::Object(tool_call.arguments.clone())),
                            partial: output.clone(),
                        });
                        sender.push(AssistantMessageEvent::ToolCallEnd {
                            content_index: index,
                            tool_call,
                            partial: output.clone(),
                        });
                    }
                }
            }
            if let Some(finish_reason) = &candidate.finish_reason {
                output.raw_stop_reason = Some(finish_reason.clone());
                output.stop_reason =
                    map_stop_reason(finish_reason).map_err(|message| GoogleRequestError::new(message, None))?;
                if output
                    .content
                    .iter()
                    .any(|block| matches!(block, AssistantContent::ToolCall(_)))
                {
                    output.stop_reason = StopReason::ToolUse;
                }
            }
        }
        if let Some(usage) = &chunk.usage_metadata {
            let cached = usage.cached_content_token_count.unwrap_or(0.0);
            let thoughts = usage.thoughts_token_count.unwrap_or(0.0);
            output.usage = Usage {
                input: usage.prompt_token_count.unwrap_or(0.0) - cached,
                output: usage.candidates_token_count.unwrap_or(0.0) + thoughts,
                cache_read: cached,
                cache_write: 0.0,
                cache_write_1h: None,
                reasoning: Some(thoughts),
                total_tokens: usage.total_token_count.unwrap_or(0.0),
                cost: UsageCost::default(),
            };
            output.usage.cost = calculate_cost(model, &output.usage);
        }
    }
    close_block(sender, output, current);
    Ok(())
}

fn open_block(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    thinking: bool,
) -> (BlockKind, usize) {
    let index = output.content.len();
    if thinking {
        output
            .content
            .push(AssistantContent::Thinking(ThinkingContent::new("")));
        sender.push(AssistantMessageEvent::ThinkingStart {
            content_index: index,
            partial: output.clone(),
        });
        (BlockKind::Thinking, index)
    } else {
        output.content.push(AssistantContent::Text(TextContent::new("")));
        sender.push(AssistantMessageEvent::TextStart {
            content_index: index,
            partial: output.clone(),
        });
        (BlockKind::Text, index)
    }
}

fn close_block(sender: &AssistantMessageEventSender, output: &AssistantMessage, current: Option<(BlockKind, usize)>) {
    let Some((kind, index)) = current else {
        return;
    };
    match kind {
        BlockKind::Text => {
            if let Some(AssistantContent::Text(text)) = output.content.get(index) {
                sender.push(AssistantMessageEvent::TextEnd {
                    content_index: index,
                    content: text.text.clone(),
                    partial: output.clone(),
                });
            }
        }
        BlockKind::Thinking => {
            if let Some(AssistantContent::Thinking(thinking)) = output.content.get(index) {
                sender.push(AssistantMessageEvent::ThinkingEnd {
                    content_index: index,
                    content: thinking.thinking.clone(),
                    partial: output.clone(),
                });
            }
        }
    }
}

fn append_text(output: &mut AssistantMessage, kind: BlockKind, index: usize, text: &str, signature: Option<&str>) {
    match (kind, output.content.get_mut(index)) {
        (BlockKind::Text, Some(AssistantContent::Text(block))) => {
            block.text.push_str(text);
            block.text_signature = retain_thought_signature(block.text_signature.as_deref(), signature);
        }
        (BlockKind::Thinking, Some(AssistantContent::Thinking(block))) => {
            block.thinking.push_str(text);
            block.thinking_signature = retain_thought_signature(block.thinking_signature.as_deref(), signature);
        }
        _ => {}
    }
}

fn generated_tool_call_id(name: &str) -> String {
    let counter = TOOL_CALL_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    format!("{name}_{}_{counter}", now_millis())
}

fn build_params(
    model: &Model,
    context: &Context,
    options: &GoogleVertexOptions,
) -> Result<Map<String, Value>, GoogleRequestError> {
    let contents = convert_messages(model, context, now_millis());
    let mut generation_config = Map::new();
    if let Some(temperature) = options.stream.temperature {
        generation_config.insert("temperature".to_owned(), json!(temperature));
    }
    if let Some(max_tokens) = options.stream.max_tokens {
        generation_config.insert("maxOutputTokens".to_owned(), json!(max_tokens));
    }

    let supports_strict_mode = supports_google_strict_tool_sampling(&model.id);
    let tools = context.tools.as_deref().filter(|tools| !tools.is_empty());
    let function_calling_mode = match tools {
        Some(tools) => {
            resolve_google_function_calling_mode(tools, options.tool_choice.as_deref(), supports_strict_mode)
                .map_err(|error| GoogleRequestError::new(error.message, None))?
        }
        None => None,
    };
    let mut config = Map::new();
    config.extend(generation_config);
    if let Some(system_prompt) = context.system_prompt.as_deref().filter(|prompt| !prompt.is_empty()) {
        config.insert(
            "systemInstruction".to_owned(),
            Value::String(sanitize_surrogates(system_prompt).to_owned()),
        );
    }
    if let Some(tools) = tools {
        let declarations = convert_tools(tools, false, supports_strict_mode)
            .map_err(|error| GoogleRequestError::new(error.message, None))?;
        if let Some(declarations) = declarations {
            config.insert("tools".to_owned(), Value::Array(declarations));
        }
    }
    if let Some(mode) = function_calling_mode {
        config.insert(
            "toolConfig".to_owned(),
            json!({"functionCallingConfig": {"mode": mode}}),
        );
    }
    if let Some(thinking_config) = thinking_config(model, options.thinking.as_ref()) {
        config.insert("thinkingConfig".to_owned(), thinking_config);
    }
    if options
        .stream
        .request
        .signal
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
    {
        return Err(GoogleRequestError::new("Request aborted", None));
    }

    let mut params = Map::new();
    params.insert("model".to_owned(), Value::String(model.id.clone()));
    params.insert(
        "contents".to_owned(),
        Value::Array(
            contents
                .into_iter()
                .map(|content| serde_json::to_value(content).unwrap_or(Value::Null))
                .collect(),
        ),
    );
    params.insert("config".to_owned(), Value::Object(config));
    Ok(params)
}

fn thinking_config(model: &Model, thinking: Option<&GoogleThinkingOptions>) -> Option<Value> {
    let thinking = thinking?;
    if thinking.enabled && model.reasoning {
        let mut config = Map::new();
        config.insert("includeThoughts".to_owned(), Value::Bool(true));
        if let Some(level) = thinking.level {
            // `THINKING_LEVEL_MAP` maps each level onto the identical Google enum value.
            config.insert("thinkingLevel".to_owned(), json!(level));
        } else if let Some(budget) = thinking.budget_tokens {
            config.insert("thinkingBudget".to_owned(), json!(budget));
        }
        return Some(Value::Object(config));
    }
    if model.reasoning && !thinking.enabled {
        return Some(get_disabled_thinking_config(model));
    }
    None
}

fn get_disabled_thinking_config(model: &Model) -> Value {
    if is_gemini3_pro_model(model) {
        return json!({"thinkingLevel": GoogleThinkingLevel::Low});
    }
    if is_gemini3_flash_model(model) {
        return json!({"thinkingLevel": GoogleThinkingLevel::Minimal});
    }
    json!({"thinkingBudget": 0})
}

fn is_gemini3_pro_model(model: &Model) -> bool {
    let id = model.id.to_lowercase();
    let Some(rest) = id.strip_prefix("gemini-3") else {
        return false;
    };
    let rest = match rest.strip_prefix('.') {
        Some(rest) => match rest.find('-') {
            Some(index) if rest[..index].chars().all(|character| character.is_ascii_digit()) => &rest[index..],
            _ => return false,
        },
        None => rest,
    };
    rest.starts_with("-pro")
}

fn is_gemini3_flash_model(model: &Model) -> bool {
    let id = model.id.to_lowercase();
    if id == "gemini-flash-latest" || id == "gemini-flash-lite-latest" {
        return true;
    }
    let Some(rest) = id.strip_prefix("gemini-3") else {
        return false;
    };
    let rest = match rest.strip_prefix('.') {
        Some(rest) => match rest.find('-') {
            Some(index) if rest[..index].chars().all(|character| character.is_ascii_digit()) => &rest[index..],
            _ => return false,
        },
        None => rest,
    };
    rest.starts_with("-flash")
}

fn effort_name(effort: ModelThinkingLevel) -> Option<&'static str> {
    match effort {
        ModelThinkingLevel::Minimal => Some("minimal"),
        ModelThinkingLevel::Low => Some("low"),
        ModelThinkingLevel::Medium => Some("medium"),
        ModelThinkingLevel::High => Some("high"),
        ModelThinkingLevel::Off | ModelThinkingLevel::Xhigh | ModelThinkingLevel::Max | ModelThinkingLevel::Ultra => {
            None
        }
    }
}

fn get_gemini3_thinking_level(effort: ModelThinkingLevel, model: &Model) -> Option<GoogleThinkingLevel> {
    let effort = effort_name(effort)?;
    if is_gemini3_pro_model(model) {
        return Some(match effort {
            "minimal" | "low" => GoogleThinkingLevel::Low,
            _ => GoogleThinkingLevel::High,
        });
    }
    Some(match effort {
        "minimal" => GoogleThinkingLevel::Minimal,
        "low" => GoogleThinkingLevel::Low,
        "medium" => GoogleThinkingLevel::Medium,
        _ => GoogleThinkingLevel::High,
    })
}

fn get_google_budget(
    model: &Model,
    effort: ModelThinkingLevel,
    custom_budgets: Option<&ThinkingBudgets>,
) -> Option<f64> {
    let effort = effort_name(effort)?;
    if let Some(budget) = custom_budgets.and_then(|budgets| match effort {
        "minimal" => budgets.minimal,
        "low" => budgets.low,
        "medium" => budgets.medium,
        _ => budgets.high,
    }) {
        return Some(budget);
    }
    let budgets: [f64; 4] = if model.id.contains("2.5-pro") {
        [128.0, 2048.0, 8192.0, 32768.0]
    } else if model.id.contains("2.5-flash") {
        [128.0, 2048.0, 8192.0, 24576.0]
    } else {
        return Some(-1.0);
    };
    Some(match effort {
        "minimal" => budgets[0],
        "low" => budgets[1],
        "medium" => budgets[2],
        _ => budgets[3],
    })
}

fn resolve_api_key(options: &GoogleVertexOptions) -> Option<String> {
    let api_key = options.stream.request.api_key.as_deref()?.trim();
    if api_key.is_empty() || api_key == GCP_VERTEX_CREDENTIALS_MARKER || is_placeholder_api_key(api_key) {
        return None;
    }
    Some(api_key.to_owned())
}

fn is_placeholder_api_key(api_key: &str) -> bool {
    api_key
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .is_some_and(|inner| !inner.is_empty() && !inner.contains('>'))
}

fn resolve_project(options: &GoogleVertexOptions) -> Result<String, String> {
    let env: Option<&ProviderEnv> = options.stream.request.env.as_ref();
    options
        .project
        .clone()
        .or_else(|| get_provider_env_value("GOOGLE_CLOUD_PROJECT", env))
        .or_else(|| get_provider_env_value("GCLOUD_PROJECT", env))
        .ok_or_else(|| {
            "Vertex AI requires a project ID. Set GOOGLE_CLOUD_PROJECT/GCLOUD_PROJECT or pass project in options."
                .to_owned()
        })
}

fn resolve_location(options: &GoogleVertexOptions) -> Result<String, String> {
    let env: Option<&ProviderEnv> = options.stream.request.env.as_ref();
    options
        .location
        .clone()
        .or_else(|| get_provider_env_value("GOOGLE_CLOUD_LOCATION", env))
        .ok_or_else(|| {
            "Vertex AI requires a location. Set GOOGLE_CLOUD_LOCATION or pass location in options.".to_owned()
        })
}
