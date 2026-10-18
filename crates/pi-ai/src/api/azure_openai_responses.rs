//! Port of `packages/ai/src/api/azure-openai-responses.ts` (+ `.lazy.ts`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::api::constrained_sampling::create_grammar_tool_input_properties;
use crate::api::lazy::{LazyApi, LazyApiCapabilities, lazy_api};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_responses_shared::{
    ConvertResponsesMessagesOptions, ConvertResponsesToolsOptions, OpenAIResponsesStreamOptions,
    convert_responses_messages, convert_responses_tools, process_responses_stream, push_responses_done,
};
use crate::api::openai_sdk::{
    AzureClientConfig, SdkClient, SdkClientConfig, SdkError, format_request_error, format_sdk_error,
};
use crate::api::simple_options::{build_base_options, clamp_standard_reasoning_level, model_thinking_level};
use crate::models::clamp_thinking_level;
use crate::request::{
    BoxFuture, CallbackError, ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis,
};
use crate::types::{
    AssistantMessage, Context, ErrorReason, Model, ModelCompat, ModelThinkingLevel, ProviderResponse, StopReason,
};
use crate::utils::event_stream::{AssistantMessageEventSender, create_assistant_message_event_stream};
use crate::utils::headers::headers_to_record;
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::provider_retry::{ProviderRequestError, ProviderRetryOptions, retry_provider_request};

const DEFAULT_AZURE_API_VERSION: &str = "v1";
const AZURE_TOOL_CALL_PROVIDERS: [&str; 4] = ["openai", "openai-codex", "opencode", "azure-openai-responses"];
const OPENAI_RESPONSES_MIN_OUTPUT_TOKENS: f64 = 16.0;

/// `AzureOpenAIResponsesOptions`.
#[derive(Debug, Clone, Default)]
pub struct AzureOpenAIResponsesOptions {
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: Option<String>,
    pub azure_api_version: Option<String>,
    pub azure_resource_name: Option<String>,
    pub azure_base_url: Option<String>,
    pub azure_deployment_name: Option<String>,
}

impl AzureOpenAIResponsesOptions {
    pub fn from_stream_options(options: &StreamOptions) -> Self {
        let field = |key: &str| options.extra.get(key).and_then(Value::as_str).map(str::to_owned);
        Self {
            reasoning_effort: field("reasoningEffort"),
            reasoning_summary: field("reasoningSummary"),
            azure_api_version: field("azureApiVersion"),
            azure_resource_name: field("azureResourceName"),
            azure_base_url: field("azureBaseUrl"),
            azure_deployment_name: field("azureDeploymentName"),
        }
    }
}

fn parse_deployment_name_map(value: Option<&str>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(value) = value else {
        return map;
    };
    for entry in value.split(',') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((model_id, deployment_name)) = trimmed.split_once('=') else {
            continue;
        };
        if model_id.is_empty() || deployment_name.is_empty() {
            continue;
        }
        map.insert(model_id.trim().to_owned(), deployment_name.trim().to_owned());
    }
    map
}

fn resolve_deployment_name(
    model: &Model,
    options: &AzureOpenAIResponsesOptions,
    env: Option<&crate::types::ProviderEnv>,
) -> String {
    if let Some(deployment_name) = &options.azure_deployment_name {
        return deployment_name.clone();
    }
    let map = parse_deployment_name_map(get_provider_env_value("AZURE_OPENAI_DEPLOYMENT_NAME_MAP", env).as_deref());
    map.get(&model.id).cloned().unwrap_or_else(|| model.id.clone())
}

fn normalize_azure_base_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    let mut url = reqwest::Url::parse(trimmed).map_err(|_| format!("Invalid Azure OpenAI base URL: {base_url}"))?;

    let host = url.host_str().unwrap_or("");
    let is_azure_host = host.ends_with(".openai.azure.com")
        || host.ends_with(".cognitiveservices.azure.com")
        || host.ends_with(".ai.azure.com");
    let normalized_path = url.path().trim_end_matches('/');
    if is_azure_host && matches!(normalized_path, "" | "/" | "/openai" | "/openai/v1/responses") {
        url.set_path("/openai/v1");
        url.set_query(None);
    }
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

fn build_default_base_url(resource_name: &str) -> String {
    format!("https://{resource_name}.openai.azure.com/openai/v1")
}

fn resolve_azure_config(
    model: &Model,
    options: &AzureOpenAIResponsesOptions,
    stream_options: &StreamOptions,
) -> Result<(String, String), String> {
    let env = stream_options.request.env.as_ref();
    let api_version = options
        .azure_api_version
        .clone()
        .or_else(|| get_provider_env_value("AZURE_OPENAI_API_VERSION", env))
        .unwrap_or_else(|| DEFAULT_AZURE_API_VERSION.to_owned());

    let base_url = options
        .azure_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            get_provider_env_value("AZURE_OPENAI_BASE_URL", env)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        });
    let resource_name = options
        .azure_resource_name
        .clone()
        .or_else(|| get_provider_env_value("AZURE_OPENAI_RESOURCE_NAME", env));

    let resolved_base_url = base_url
        .or_else(|| resource_name.as_deref().map(build_default_base_url))
        .or_else(|| (!model.base_url.is_empty()).then(|| model.base_url.clone()))
        .ok_or_else(|| {
            "Azure OpenAI base URL is required. Set AZURE_OPENAI_BASE_URL or AZURE_OPENAI_RESOURCE_NAME, or pass azureBaseUrl, azureResourceName, or model.baseUrl.".to_owned()
        })?;

    Ok((normalize_azure_base_url(&resolved_base_url)?, api_version))
}

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

fn mapped_effort(model: &Model, effort: &str) -> String {
    thinking_level_from_str(effort)
        .and_then(|level| {
            model
                .thinking_level_map
                .as_ref()
                .and_then(|map| map.get(&level))
                .cloned()
                .flatten()
        })
        .unwrap_or_else(|| effort.to_owned())
}

fn off_effort(model: &Model) -> Option<String> {
    let Some(map) = model.thinking_level_map.as_ref() else {
        return Some("none".to_owned());
    };
    match map.get(&ModelThinkingLevel::Off) {
        Some(Some(value)) => Some(value.clone()),
        Some(None) => None,
        None => Some("none".to_owned()),
    }
}

pub fn build_params(
    model: &Model,
    context: &Context,
    options: &AzureOpenAIResponsesOptions,
    stream_options: &StreamOptions,
    deployment_name: &str,
    grammar_tool_input_properties: &BTreeMap<String, String>,
) -> Result<Value, String> {
    let messages = convert_responses_messages(
        model,
        context,
        &AZURE_TOOL_CALL_PROVIDERS
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>(),
        Some(&ConvertResponsesMessagesOptions {
            grammar_tool_input_properties: Some(grammar_tool_input_properties.clone()),
            ..Default::default()
        }),
    )?;

    let mut params = Map::new();
    params.insert("model".to_owned(), Value::String(deployment_name.to_owned()));
    params.insert("input".to_owned(), Value::Array(messages));
    params.insert("stream".to_owned(), Value::Bool(true));
    if let Some(key) = clamp_openai_prompt_cache_key(stream_options.session_id.as_deref()) {
        params.insert("prompt_cache_key".to_owned(), Value::String(key.to_owned()));
    }
    params.insert("store".to_owned(), Value::Bool(false));

    if let Some(max_tokens) = stream_options.max_tokens.filter(|max_tokens| *max_tokens != 0.0) {
        params.insert(
            "max_output_tokens".to_owned(),
            number_value(max_tokens.max(OPENAI_RESPONSES_MIN_OUTPUT_TOKENS)),
        );
    }
    if let Some(temperature) = stream_options.temperature {
        params.insert("temperature".to_owned(), number_value(temperature));
    }
    if let Some(tools) = context.tools.as_deref().filter(|tools| !tools.is_empty()) {
        let supports_strict_mode = match &model.compat {
            Some(ModelCompat::OpenAIResponses(compat)) => compat.supports_strict_mode.unwrap_or(true),
            _ => true,
        };
        let supports_openai_grammar_tools = match &model.compat {
            Some(ModelCompat::OpenAIResponses(compat)) => compat.supports_openai_grammar_tools.unwrap_or(false),
            _ => false,
        };
        params.insert(
            "tools".to_owned(),
            convert_responses_tools(
                tools,
                Some(&ConvertResponsesToolsOptions {
                    supports_strict_mode: Some(supports_strict_mode),
                    supports_openai_grammar_tools: Some(supports_openai_grammar_tools),
                    ..Default::default()
                }),
            )
            .map_err(|error| error.message)?,
        );
    }

    if model.reasoning {
        if options.reasoning_effort.is_some() || options.reasoning_summary.is_some() {
            let effort = options
                .reasoning_effort
                .as_deref()
                .map(|effort| mapped_effort(model, effort))
                .unwrap_or_else(|| "medium".to_owned());
            let summary = options.reasoning_summary.clone().unwrap_or_else(|| "auto".to_owned());
            params.insert(
                "reasoning".to_owned(),
                crate::api::openai_sdk::json_object(vec![
                    ("effort", Value::String(effort)),
                    ("summary", Value::String(summary)),
                ]),
            );
            params.insert(
                "include".to_owned(),
                Value::Array(vec![Value::String("reasoning.encrypted_content".to_owned())]),
            );
        } else if let Some(effort) = off_effort(model) {
            params.insert(
                "reasoning".to_owned(),
                crate::api::openai_sdk::json_object(vec![("effort", Value::String(effort))]),
            );
        }
    }

    if let Some(sampling_params) = &stream_options.sampling_params {
        for (key, value) in sampling_params {
            params.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Object(params))
}

fn number_value(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn merge_headers(
    model_headers: Option<&BTreeMap<String, String>>,
    options_headers: Option<&crate::types::ProviderHeaders>,
) -> BTreeMap<String, Option<String>> {
    let mut headers: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (name, value) in model_headers.into_iter().flatten() {
        headers.insert(name.clone(), Some(value.clone()));
    }
    for (name, value) in options_headers.into_iter().flatten() {
        headers.insert(name.clone(), value.clone());
    }
    headers
}

fn create_client(
    model: &Model,
    api_key: &str,
    options: &AzureOpenAIResponsesOptions,
    stream_options: &StreamOptions,
) -> Result<SdkClient, String> {
    let (base_url, api_version) = resolve_azure_config(model, options, stream_options)?;
    Ok(SdkClient::new(SdkClientConfig {
        api_key: api_key.to_owned(),
        base_url,
        default_headers: merge_headers(model.headers.as_ref(), stream_options.request.headers.as_ref()),
        timeout_ms: stream_options.request.timeout_ms,
        fetch: stream_options.request.fetch.clone(),
        azure: Some(AzureClientConfig { api_version }),
    }))
}

enum Failure {
    Sdk(SdkError),
    Request(Box<ProviderRequestError<CallbackError>>),
    Message(String),
}

impl Failure {
    fn format(self) -> String {
        match self {
            Self::Sdk(error) => format_sdk_error(&error, Some("Azure OpenAI API error")),
            Self::Request(error) => format_request_error(&error, Some("Azure OpenAI API error")),
            Self::Message(message) => message,
        }
    }
}

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
    options: &AzureOpenAIResponsesOptions,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
) -> Result<(), Failure> {
    let deployment_name = resolve_deployment_name(model, options, stream_options.request.env.as_ref());
    let api_key = stream_options
        .request
        .api_key
        .clone()
        .ok_or_else(|| Failure::Message(format!("No API key for provider: {}", model.provider)))?;
    let client = create_client(model, &api_key, options, stream_options).map_err(Failure::Message)?;
    let supports_grammar = match &model.compat {
        Some(ModelCompat::OpenAIResponses(compat)) => compat.supports_openai_grammar_tools.unwrap_or(false),
        _ => false,
    };
    let grammar_tool_input_properties =
        create_grammar_tool_input_properties(context.tools.as_deref(), supports_grammar)
            .map_err(|error| Failure::Message(error.message))?;
    let mut params = build_params(
        model,
        context,
        options,
        stream_options,
        &deployment_name,
        &grammar_tool_input_properties,
    )
    .map_err(Failure::Message)?;

    if let Some(on_payload) = &stream_options.request.on_payload
        && let Some(next_params) = on_payload(params.clone(), model.clone())
            .await
            .map_err(|error| Failure::Message(error.message))?
    {
        params = next_params;
    }

    let signal = stream_options.request.signal.clone();
    let response = retry_provider_request(
        || client.create_response_stream(params.clone(), signal.clone()),
        retry_options(&stream_options.request),
    )
    .await
    .map_err(|error| Failure::Request(Box::new(error)))?;

    if let Some(on_response) = &stream_options.request.on_response {
        on_response(
            ProviderResponse {
                status: f64::from(response.status),
                headers: headers_to_record(response.headers.clone()),
            },
            model.clone(),
        )
        .await
        .map_err(|error| Failure::Message(error.message))?;
    }
    sender.push(crate::types::AssistantMessageEvent::Start {
        partial: output.clone(),
    });

    let mut stream = response.stream;
    let shared_options = OpenAIResponsesStreamOptions {
        grammar_tool_input_properties: Some(grammar_tool_input_properties),
        ..Default::default()
    };
    process_responses_stream(&mut stream, output, sender, model, Some(&shared_options))
        .await
        .map_err(|failure| match failure {
            crate::api::openai_responses_shared::ResponsesStreamFailure::Source(error) => Failure::Sdk(error),
            crate::api::openai_responses_shared::ResponsesStreamFailure::Message(message) => Failure::Message(message),
        })?;

    if stream_options
        .request
        .signal
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
    {
        return Err(Failure::Message("Request was aborted".to_owned()));
    }
    if output.stop_reason == StopReason::Pending {
        return Err(Failure::Message(
            "Azure OpenAI Responses stream ended without a stop reason".to_owned(),
        ));
    }
    if matches!(output.stop_reason, StopReason::Aborted | StopReason::Error) {
        return Err(Failure::Message(
            output
                .error_message
                .clone()
                .unwrap_or_else(|| "An unknown error occurred".to_owned()),
        ));
    }
    push_responses_done(sender, output).map_err(Failure::Message)?;
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
        let options = AzureOpenAIResponsesOptions::from_stream_options(&stream_options);
        let mut output = AssistantMessage::empty("azure-openai-responses", &model.provider, &model.id, now_millis());
        let result = run_stream(&model, &context, &stream_options, &options, &mut output, &sender).await;
        if let Err(failure) = result {
            output.stop_reason = if stream_options
                .request
                .signal
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            {
                StopReason::Aborted
            } else {
                StopReason::Error
            };
            output.error_message = Some(failure.format());
            sender.push(crate::types::AssistantMessageEvent::Error {
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
    let Some(api_key) = options.stream.request.api_key.clone() else {
        return crate::request::error_stream(&model, &format!("No API key for provider: {}", model.provider));
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
    run_and_emit(&model, &context, stream_options)
}

/// `ProviderStreams` adapter for `azureOpenAIResponsesApi()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct AzureOpenAIResponsesApi;

impl ProviderStreams for AzureOpenAIResponsesApi {
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> crate::utils::event_stream::AssistantMessageEventStream {
        stream(model, context, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> crate::utils::event_stream::AssistantMessageEventStream {
        stream_simple(model, context, options)
    }
}

/// `azureOpenAIResponsesApi()` from `azure-openai-responses.lazy.ts`.
pub fn azure_open_ai_responses_api() -> LazyApi {
    lazy_api(
        Arc::new(|| {
            Box::pin(async { Ok(Arc::new(AzureOpenAIResponsesApi) as Arc<dyn ProviderStreams>) })
                as BoxFuture<'static, Result<Arc<dyn ProviderStreams>, CallbackError>>
        }),
        LazyApiCapabilities::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(base_url: &str) -> Model {
        serde_json::from_value(serde_json::json!({
            "id": "gpt-4.1", "name": "gpt-4.1", "api": "azure-openai-responses",
            "provider": "azure-openai-responses", "baseUrl": base_url, "reasoning": false,
            "input": ["text"],
            "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0},
            "contextWindow": 128000, "maxTokens": 4096
        }))
        .expect("model")
    }

    #[test]
    fn azure_hosts_get_the_v1_base_path() {
        assert_eq!(
            normalize_azure_base_url("https://example.openai.azure.com/").unwrap(),
            "https://example.openai.azure.com/openai/v1"
        );
        assert_eq!(
            normalize_azure_base_url("https://example.openai.azure.com/openai/v1/responses").unwrap(),
            "https://example.openai.azure.com/openai/v1"
        );
        assert_eq!(
            normalize_azure_base_url("https://proxy.example.com/custom").unwrap(),
            "https://proxy.example.com/custom"
        );
    }

    #[test]
    fn deployment_name_map_parses_entries() {
        let map = parse_deployment_name_map(Some(" gpt-4.1=dep-a, gpt-5=dep-b , broken"));
        assert_eq!(map.get("gpt-4.1"), Some(&"dep-a".to_owned()));
        assert_eq!(map.get("gpt-5"), Some(&"dep-b".to_owned()));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn resource_name_builds_default_base_url() {
        assert_eq!(
            build_default_base_url("example"),
            "https://example.openai.azure.com/openai/v1"
        );
        assert_eq!(model("https://example.openai.azure.com").id, "gpt-4.1");
    }
}
