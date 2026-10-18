//! Port of `packages/ai/src/api/google-generative-ai.ts`.

use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::api::google_client::GoogleClient;
use crate::api::google_shared::{
    GoogleRequestError, GoogleRetryOptions, GoogleThinkingLevel, convert_messages, convert_tools,
    resolve_google_function_calling_mode, retry_google_request, supports_google_strict_tool_sampling,
};
use crate::api::lazy::{LazyApi, lazy_api};
use crate::api::simple_options::build_base_options;
use crate::http::default_fetch;
use crate::models::clamp_thinking_level;
use crate::request::{ProviderStreams, SimpleStreamOptions, StreamOptions, error_stream, now_millis};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, Context, DoneReason, ErrorReason, Model, ModelThinkingLevel, StopReason,
    ThinkingBudgets,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::sanitize_unicode::sanitize_surrogates;

/// `GoogleOptions` from the TypeScript adapter.
#[derive(Clone, Default)]
pub struct GoogleOptions {
    pub stream: StreamOptions,
    pub tool_choice: Option<String>,
    pub thinking: Option<GoogleThinkingOptions>,
}

/// `thinking?: { enabled, budgetTokens?, level? }`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleThinkingOptions {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub budget_tokens: Option<f64>,
    #[serde(default)]
    pub level: Option<GoogleThinkingLevel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleOptionsExtra {
    #[serde(default)]
    tool_choice: Option<String>,
    #[serde(default)]
    thinking: Option<GoogleThinkingOptions>,
}

impl GoogleOptions {
    /// Parse the API-specific fields carried by [`StreamOptions::extra`].
    pub fn from_stream_options(stream: StreamOptions) -> Result<Self, String> {
        let extra: GoogleOptionsExtra =
            serde_json::from_value(Value::Object(stream.extra.clone())).map_err(|error| error.to_string())?;
        Ok(Self {
            stream,
            tool_choice: extra.tool_choice,
            thinking: extra.thinking,
        })
    }
}

/// `stream()`.
pub fn stream(model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
    let options = match GoogleOptions::from_stream_options(options) {
        Ok(options) => options,
        Err(message) => return error_stream(&model, &message),
    };
    run_stream(model, context, options)
}

/// `streamSimple()`.
pub fn stream_simple(model: Model, context: Context, options: SimpleStreamOptions) -> AssistantMessageEventStream {
    let Some(api_key) = options.stream.request.api_key.clone() else {
        return error_stream(&model, &format!("No API key for provider: {}", model.provider));
    };
    let base = build_base_options(&model, &context, Some(&options), Some(&api_key));
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
            if is_gemini3_pro_model(&model) || is_gemini3_flash_model(&model) || is_gemma4_model(&model) {
                Some(GoogleThinkingOptions {
                    enabled: true,
                    budget_tokens: None,
                    level: get_thinking_level(effort, &model),
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
    run_stream(
        model,
        context,
        GoogleOptions {
            stream: base,
            tool_choice: None,
            thinking,
        },
    )
}

/// `ProviderStreams` adapter for `googleGenerativeAIApi()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct GoogleGenerativeAiApi;

impl ProviderStreams for GoogleGenerativeAiApi {
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

/// `googleGenerativeAIApi()` from `google-generative-ai.lazy.ts`.
pub fn google_generative_ai_api() -> LazyApi {
    lazy_api(
        std::sync::Arc::new(|| {
            Box::pin(async { Ok(std::sync::Arc::new(GoogleGenerativeAiApi) as std::sync::Arc<dyn ProviderStreams>) })
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

fn run_stream(model: Model, context: Context, options: GoogleOptions) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    let signal = options.stream.request.signal.clone();
    tokio::spawn(async move {
        let mut output = AssistantMessage::empty(
            "google-generative-ai",
            model.provider.clone(),
            model.id.clone(),
            now_millis(),
        );
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
    options: &GoogleOptions,
    signal: Option<&CancellationToken>,
) -> Result<(), String> {
    let Some(api_key) = options.stream.request.api_key.clone() else {
        return Err(format!("No API key for provider: {}", model.provider));
    };
    let fetch = options.stream.request.fetch.clone().unwrap_or_else(default_fetch);
    let client = GoogleClient::generative_ai(model, &api_key, options.stream.request.headers.as_ref());
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
    crate::api::google_client::pump_stream(&mut google_stream, output, sender, model)
        .await
        .map_err(|error| error.message)?;

    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err("Request was aborted".to_owned());
    }
    if output.stop_reason == StopReason::Pending {
        return Err("Google stream ended without a finish reason".to_owned());
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

fn build_params(
    model: &Model,
    context: &Context,
    options: &GoogleOptions,
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
    let thinking_config = thinking_config(model, options.thinking.as_ref());
    if let Some(thinking_config) = thinking_config {
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
    if is_gemini3_flash_model(model) || is_gemma4_model(model) {
        return json!({"thinkingLevel": GoogleThinkingLevel::Minimal});
    }
    json!({"thinkingBudget": 0})
}

fn is_gemma4_model(model: &Model) -> bool {
    is_gemma4_id(&model.id)
}

fn is_gemma4_id(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
    id.contains("gemma4") || id.contains("gemma-4")
}

fn is_gemini3_pro_model(model: &Model) -> bool {
    is_gemini3_pro_id(&model.id)
}

fn is_gemini3_pro_id(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
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

fn get_thinking_level(effort: ModelThinkingLevel, model: &Model) -> Option<GoogleThinkingLevel> {
    let effort = effort_name(effort)?;
    if is_gemini3_pro_model(model) {
        return Some(match effort {
            "minimal" | "low" => GoogleThinkingLevel::Low,
            _ => GoogleThinkingLevel::High,
        });
    }
    if is_gemma4_model(model) {
        return Some(match effort {
            "minimal" | "low" => GoogleThinkingLevel::Minimal,
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
    } else if model.id.contains("2.5-flash-lite") {
        [512.0, 2048.0, 8192.0, 24576.0]
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
