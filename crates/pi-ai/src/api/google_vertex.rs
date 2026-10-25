//! Port of `packages/ai/src/api/google-vertex.ts`.

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::api::google_client::{GoogleClient, completed_reason, open_generate_stream, pump_stream};
use crate::api::google_generative_ai::GoogleThinkingOptions;
use crate::api::google_shared::{
    GenerateContentParamsInput, GoogleRequestError, GoogleThinkingLevel, build_generate_content_params,
    is_gemini3_flash_id, is_gemini3_pro_id, model_thinking_level, thinking_effort_name,
};
use crate::api::lazy::{LazyApi, lazy_api};
use crate::api::simple_options::build_base_options;
use crate::models::clamp_thinking_level;
use crate::request::{ProviderStreams, SimpleStreamOptions, StreamOptions, error_stream, now_millis};
use crate::types::{
    AssistantMessage, AssistantMessageEvent, Context, ErrorReason, Model, ModelThinkingLevel, ProviderEnv, StopReason,
    ThinkingBudgets,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::provider_env::get_provider_env_value;

const GCP_VERTEX_CREDENTIALS_MARKER: &str = "gcp-vertex-credentials";

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
            if is_gemini3_pro_id(&model.id) || is_gemini3_flash_id(&model.id) {
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
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(stream(model, context, options))
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(stream_simple(model, context, options))
    }
}

/// `googleVertexApi()` from `google-vertex.lazy.ts`.
pub fn google_vertex_api() -> LazyApi {
    lazy_api(std::sync::Arc::new(|| {
        Box::pin(async { Ok(std::sync::Arc::new(GoogleVertexApi) as std::sync::Arc<dyn ProviderStreams>) })
    }))
}

fn run_stream(model: Model, context: Context, options: GoogleVertexOptions) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    let signal = options.stream.request.signal.clone();
    tokio::spawn(async move {
        let mut output = AssistantMessage::empty(
            "google-vertex",
            model.provider.clone(),
            model.id.clone(),
            now_millis() as f64,
        );
        match run(&sender, &mut output, &model, &context, &options, signal.as_ref()).await {
            Ok(()) => {}
            Err(message) => {
                let reason = if signal.as_ref().is_some_and(AbortSignal::aborted) {
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
    signal: Option<&AbortSignal>,
) -> Result<(), pi_js_compat::js_json::JsString> {
    let client = resolve_client(model, options)?;
    let params = prepare_params(model, context, options).await?;
    let mut google_stream = open_generate_stream(&client, &params, &options.stream.request)
        .await
        .map_err(|error| error.message)?;

    sender.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    pump_stream(&mut google_stream, output, sender, model)
        .await
        .map_err(|error| error.message)?;

    let reason = completed_reason(output, signal)?;
    sender.push(AssistantMessageEvent::Done {
        reason,
        message: output.clone(),
    });
    sender.end(None);
    Ok(())
}

fn resolve_client(
    model: &Model,
    options: &GoogleVertexOptions,
) -> Result<GoogleClient, pi_js_compat::js_json::JsString> {
    if let Some(api_key) = resolve_api_key(options) {
        return Ok(GoogleClient::vertex_api_key(
            model,
            &api_key,
            options.stream.request.headers.as_ref(),
        ));
    }
    let project = project_from_options_or_env(options)?;
    let location = location_from_options_or_env(options)?;
    GoogleClient::vertex_adc(
        model,
        &project,
        &location,
        options.stream.request.headers.as_ref(),
        options.stream.request.env.as_ref(),
    )
    .map_err(|error| error.message)
}

async fn prepare_params(
    model: &Model,
    context: &Context,
    options: &GoogleVertexOptions,
) -> Result<JsValue, pi_js_compat::js_json::JsString> {
    let params = JsValue::Object(build_params(model, context, options).map_err(|error| error.message)?);
    let Some(on_payload) = &options.stream.request.on_payload else {
        return Ok(params);
    };
    on_payload(params.clone(), model.clone())
        .await
        .map_err(|error| super::openai_sdk::format_callback_error(&error, None))
        .map(|replacement| replacement.unwrap_or(params))
}

fn build_params(
    model: &Model,
    context: &Context,
    options: &GoogleVertexOptions,
) -> Result<JsObject, GoogleRequestError> {
    let request_aborted = options.stream.request.signal.as_ref().is_some_and(AbortSignal::aborted);
    build_generate_content_params(GenerateContentParamsInput {
        model,
        context,
        temperature: options.stream.temperature,
        max_tokens: options.stream.max_tokens,
        tool_choice: options.tool_choice.as_deref(),
        thinking_config: thinking_config(model, options.thinking.as_ref()),
        request_aborted,
        now: now_millis() as f64,
    })
}

fn thinking_config(model: &Model, thinking: Option<&GoogleThinkingOptions>) -> Option<Value> {
    if crate::api::google_shared::uses_server_managed_thinking(&model.id) {
        return None;
    }
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
    if is_gemini3_pro_id(&model.id) || model.id == "gemini-3.7-flash" {
        return json!({"thinkingLevel": GoogleThinkingLevel::Low});
    }
    if is_gemini3_flash_id(&model.id) {
        return json!({"thinkingLevel": GoogleThinkingLevel::Minimal});
    }
    json!({"thinkingBudget": 0})
}

fn get_gemini3_thinking_level(effort: ModelThinkingLevel, model: &Model) -> Option<GoogleThinkingLevel> {
    let effort = thinking_effort_name(effort)?;
    if is_gemini3_pro_id(&model.id) {
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
    let effort = thinking_effort_name(effort)?;
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

fn resolve_api_key(options: &GoogleVertexOptions) -> Option<JsString> {
    let api_key = options.stream.request.api_key.as_ref()?.trim();
    if api_key.is_empty() || api_key == GCP_VERTEX_CREDENTIALS_MARKER || is_placeholder_api_key(&api_key) {
        return None;
    }
    Some(api_key)
}

fn is_placeholder_api_key(api_key: &JsString) -> bool {
    api_key
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .is_some_and(|inner| !inner.is_empty() && !inner.contains('>'))
}

fn project_from_options_or_env(options: &GoogleVertexOptions) -> Result<JsString, String> {
    let env: Option<&ProviderEnv> = options.stream.request.env.as_ref();
    options
        .project
        .clone()
        .filter(|project| !project.is_empty())
        .map(JsString::from)
        .or_else(|| get_provider_env_value("GOOGLE_CLOUD_PROJECT", env))
        .or_else(|| get_provider_env_value("GCLOUD_PROJECT", env))
        .ok_or_else(|| {
            "Vertex AI requires a project ID. Set GOOGLE_CLOUD_PROJECT/GCLOUD_PROJECT or pass project in options."
                .to_owned()
        })
}

fn location_from_options_or_env(options: &GoogleVertexOptions) -> Result<JsString, String> {
    let env: Option<&ProviderEnv> = options.stream.request.env.as_ref();
    options
        .location
        .clone()
        .filter(|location| !location.is_empty())
        .map(JsString::from)
        .or_else(|| get_provider_env_value("GOOGLE_CLOUD_LOCATION", env))
        .ok_or_else(|| {
            "Vertex AI requires a location. Set GOOGLE_CLOUD_LOCATION or pass location in options.".to_owned()
        })
}
