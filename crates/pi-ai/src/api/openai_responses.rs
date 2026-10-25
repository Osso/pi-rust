//! Port of `packages/ai/src/api/openai-responses.ts` (+ `.lazy.ts`).

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::{JsObject, JsValue};
use std::sync::Arc;

use indexmap::IndexMap;
use pi_js_compat::abort::AbortSignal;
use serde_json::{Map, Value};

use crate::api::github_copilot_headers::{build_copilot_dynamic_headers, has_copilot_vision_input};
use crate::api::lazy::{LazyApi, lazy_api};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_responses_shared::{
    OpenAIResponsesStreamOptions, convert_responses_messages, convert_responses_tools, process_responses_stream,
    push_responses_done,
};
use crate::api::openai_sdk::{SdkClient, SdkClientConfig, SdkError, format_request_error, format_sdk_error};
use crate::api::simple_options::{build_base_options, clamp_standard_reasoning_level, model_thinking_level};
use crate::models::clamp_thinking_level;
use crate::request::{
    BoxFuture, CallbackError, ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis,
};
use crate::types::{
    AssistantMessage, CacheRetention, CompatFields, Context, ErrorReason, Model, ModelCompat, ModelThinkingLevel,
    ProviderEnv, ProviderHeaders, ProviderResponse, Usage,
};
use crate::utils::event_stream::{AssistantMessageEventSender, create_assistant_message_event_stream};
use crate::utils::headers::headers_to_record;
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::provider_retry::{ProviderRequestError, ProviderRetryOptions, retry_provider_request};
use pi_js_compat::number::js_number_value;

const OPENAI_TOOL_CALL_PROVIDERS: [&str; 3] = ["openai", "openai-codex", "opencode"];

/// `OpenAIResponsesOptions`.
#[derive(Debug, Clone, Default)]
pub struct OpenAIResponsesOptions {
    pub reasoning_effort: Option<String>,
    pub reasoning_summary: Option<String>,
    pub service_tier: Option<String>,
    pub tool_choice: Option<Value>,
}

impl OpenAIResponsesOptions {
    pub fn from_stream_options(options: &StreamOptions) -> Self {
        Self {
            reasoning_effort: string_field(&options.extra, "reasoningEffort"),
            reasoning_summary: string_field(&options.extra, "reasoningSummary"),
            service_tier: string_field(&options.extra, "serviceTier"),
            tool_choice: options.extra.get("toolChoice").cloned(),
        }
    }
}

fn string_field(extra: &Map<String, Value>, key: &str) -> Option<String> {
    extra.get(key).and_then(Value::as_str).map(str::to_owned)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedCompat {
    send_session_id_header: bool,
    supports_long_cache_retention: bool,
}

pub fn get_compat(model: &Model) -> ResolvedCompat {
    let compat = match &model.compat {
        Some(ModelCompat::OpenAIResponses(compat)) => compat.clone(),
        _ => Default::default(),
    };
    // Frozen60-62 nullish defaults, then truthiness at :70/:201. Role uses
    // !==false separately in shared messages (:268), not this resolved view.
    ResolvedCompat {
        send_session_id_header: compat.truthy("sendSessionIdHeader", true),
        supports_long_cache_retention: compat.truthy("supportsLongCacheRetention", true),
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

fn resolve_cache_retention(cache_retention: Option<CacheRetention>, env: Option<&ProviderEnv>) -> CacheRetention {
    if let Some(cache_retention) = cache_retention {
        return cache_retention;
    }
    if get_provider_env_value("PI_CACHE_RETENTION", env).is_some_and(|value| value == "long") {
        return CacheRetention::Long;
    }
    CacheRetention::Short
}

fn get_prompt_cache_retention(compat: &ResolvedCompat, cache_retention: CacheRetention) -> Option<&'static str> {
    (cache_retention == CacheRetention::Long && compat.supports_long_cache_retention).then_some("24h")
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

// Frozen253: selected native string ?? requested effort, without scalar projection.
fn mapped_effort(model: &Model, effort: &str) -> JsString {
    thinking_level_from_str(effort)
        .and_then(|level| {
            model
                .thinking_level_map
                .as_ref()
                .and_then(|map| map.get(&level))
                .cloned()
                .flatten()
        })
        .unwrap_or_else(|| effort.into())
}

// Frozen260-262: explicit null disables; absent off defaults to "none".
fn off_effort(model: &Model) -> Option<JsString> {
    let Some(map) = model.thinking_level_map.as_ref() else {
        return Some("none".into());
    };
    match map.get(&ModelThinkingLevel::Off) {
        Some(Some(value)) => Some(value.clone()),
        Some(None) => None,
        None => Some("none".into()),
    }
}

pub fn build_params(
    model: &Model,
    context: &Context,
    options: &OpenAIResponsesOptions,
    stream_options: &StreamOptions,
    compat: &ResolvedCompat,
) -> Result<JsValue, String> {
    let messages = convert_responses_messages(
        model,
        context,
        &OPENAI_TOOL_CALL_PROVIDERS.into_iter().map(str::to_owned).collect(),
        None,
    )?;

    let cache_retention = resolve_cache_retention(stream_options.cache_retention, stream_options.request.env.as_ref());
    let mut params = JsObject::new();
    params.insert("model".into(), JsValue::String(model.id.clone().into()));
    params.insert("input".into(), JsValue::Array(messages));
    params.insert("stream".into(), JsValue::Bool(true));
    if cache_retention != CacheRetention::None
        && let Some(key) = clamp_openai_prompt_cache_key(stream_options.session_id.as_ref())
    {
        params.insert("prompt_cache_key".into(), JsValue::String(key));
    }
    if let Some(retention) = get_prompt_cache_retention(compat, cache_retention) {
        params.insert("prompt_cache_retention".into(), JsValue::String(retention.into()));
    }
    params.insert("store".into(), JsValue::Bool(false));

    if let Some(max_tokens) = stream_options
        .max_tokens
        .filter(|max_tokens| *max_tokens != 0.0 && !max_tokens.is_nan())
    {
        params.insert("max_output_tokens".into(), js_number_value(max_tokens).into());
    }
    if let Some(temperature) = stream_options.temperature {
        params.insert("temperature".into(), js_number_value(temperature).into());
    }
    if let Some(service_tier) = &options.service_tier {
        params.insert("service_tier".into(), JsValue::String(service_tier.clone().into()));
    }
    if let Some(tools) = context.tools.as_deref().filter(|tools| !tools.is_empty()) {
        params.insert("tools".into(), convert_responses_tools(tools, None));
    }
    if let Some(tool_choice) = &options.tool_choice {
        params.insert("tool_choice".into(), tool_choice.clone().into());
    }

    if model.reasoning {
        if options.reasoning_effort.is_some() || options.reasoning_summary.is_some() {
            let effort = options
                .reasoning_effort
                .as_deref()
                .map(|effort| mapped_effort(model, effort))
                .unwrap_or_else(|| "medium".into());
            let summary = options.reasoning_summary.clone().unwrap_or_else(|| "auto".into());
            params.insert(
                "reasoning".into(),
                JsValue::object(vec![
                    ("effort", JsValue::String(effort)),
                    ("summary", JsValue::String(summary.into())),
                ]),
            );
            params.insert(
                "include".into(),
                JsValue::Array(vec![JsValue::String("reasoning.encrypted_content".into())]),
            );
        } else if model.provider != "github-copilot"
            && let Some(effort) = off_effort(model)
        {
            params.insert(
                "reasoning".into(),
                JsValue::object(vec![("effort", JsValue::String(effort))]),
            );
        }
    }

    if let Some(sampling_params) = &stream_options.sampling_params {
        for (key, value) in sampling_params {
            params.insert(key.clone().into(), value.clone().into());
        }
    }
    Ok(JsValue::Object(params))
}

fn get_service_tier_cost_multiplier(model_id: &str, service_tier: Option<&str>) -> f64 {
    match service_tier {
        Some("flex") => 0.5,
        Some("priority") if model_id == "gpt-5.5" => 2.5,
        Some("priority") => 2.0,
        _ => 1.0,
    }
}

fn apply_service_tier_pricing(usage: &mut Usage, service_tier: Option<&str>, model_id: &str) {
    let multiplier = get_service_tier_cost_multiplier(model_id, service_tier);
    if multiplier == 1.0 {
        return;
    }
    usage.cost.input *= multiplier;
    usage.cost.output *= multiplier;
    usage.cost.cache_read *= multiplier;
    usage.cost.cache_write *= multiplier;
    usage.cost.total = usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
}

fn merge_provider_headers(
    base: Option<&IndexMap<String, JsString>>,
    dynamic: Option<IndexMap<String, String>>,
    options_headers: Option<&ProviderHeaders>,
) -> IndexMap<String, Option<JsString>> {
    let mut merged: IndexMap<String, Option<JsString>> = IndexMap::new();
    for (name, value) in base.into_iter().flatten() {
        merged.insert(name.clone(), Some(value.clone()));
    }
    for (name, value) in dynamic.into_iter().flatten() {
        merged.insert(name, Some(value.into()));
    }
    for (name, value) in options_headers.into_iter().flatten() {
        merged.insert(name.clone(), value.clone());
    }
    merged
}

fn create_client(
    model: &Model,
    context: &Context,
    api_key: &JsString,
    options: &StreamOptions,
    session_id: Option<&JsString>,
    compat: &ResolvedCompat,
) -> SdkClient {
    let dynamic = (model.provider == "github-copilot").then(|| {
        let has_images = has_copilot_vision_input(&context.messages);
        build_copilot_dynamic_headers(&context.messages, has_images)
    });
    let mut headers = merge_provider_headers(model.headers.as_ref(), dynamic, None);
    if let Some(session_id) = session_id {
        if compat.send_session_id_header {
            headers.insert("session_id".to_owned(), Some(session_id.clone()));
        }
        headers.insert("x-client-request-id".to_owned(), Some(session_id.clone()));
    }
    for (name, value) in options.request.headers.iter().flatten() {
        headers.insert(name.clone(), value.clone());
    }
    SdkClient::new(SdkClientConfig {
        api_key: api_key.to_owned(),
        base_url: model.base_url.clone(),
        default_headers: headers,
        timeout_ms: options.request.timeout_ms,
        fetch: options.request.fetch.clone(),
        azure: None,
    })
}

enum Failure {
    Sdk(SdkError),
    Request(Box<ProviderRequestError<CallbackError>>),
    Message(pi_js_compat::js_json::JsString),
}

impl Failure {
    fn format(self) -> pi_js_compat::js_json::JsString {
        match self {
            Self::Sdk(error) => format_sdk_error(&error, Some("OpenAI API error")),
            Self::Request(error) => format_request_error(&error, Some("OpenAI API error")),
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
    options: &OpenAIResponsesOptions,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
) -> Result<(), Failure> {
    let api_key = get_client_api_key(
        &model.provider,
        stream_options.request.api_key.as_ref(),
        stream_options.request.headers.as_ref(),
    )
    .map_err(|error| Failure::Message(error.into()))?;
    let cache_retention = resolve_cache_retention(stream_options.cache_retention, stream_options.request.env.as_ref());
    let cache_session_id = (cache_retention != CacheRetention::None)
        .then_some(stream_options.session_id.as_ref())
        .flatten();
    let compat = get_compat(model);
    let client = create_client(model, context, &api_key, stream_options, cache_session_id, &compat);
    let params = build_params(model, context, options, stream_options, &compat)
        .map_err(|error| Failure::Message(error.into()))?;

    let mut params = params;
    if let Some(on_payload) = &stream_options.request.on_payload
        && let Some(next_params) = on_payload(params.clone(), model.clone()).await.map_err(|error| {
            Failure::Message(super::openai_sdk::format_callback_error(
                &error,
                Some("OpenAI API error"),
            ))
        })?
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
            async move { client.create_response_stream(params, signal).await }
        },
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
        .map_err(|error| {
            Failure::Message(super::openai_sdk::format_callback_error(
                &error,
                Some("OpenAI API error"),
            ))
        })?;
    }
    sender.push(crate::types::AssistantMessageEvent::Start {
        partial: output.clone(),
    });

    let mut stream = response.stream;
    let stream_options_for_shared = OpenAIResponsesStreamOptions {
        service_tier: options.service_tier.clone(),
        apply_service_tier_pricing: Some(Arc::new({
            let model_id = model.id.clone();
            move |usage: &mut Usage, service_tier: Option<&str>| {
                apply_service_tier_pricing(usage, service_tier, &model_id);
            }
        })),
        ..Default::default()
    };
    process_responses_stream(&mut stream, output, sender, model, Some(&stream_options_for_shared))
        .await
        .map_err(|failure| match failure {
            crate::api::openai_responses_shared::ResponsesStreamFailure::Source(error) => Failure::Sdk(error),
            crate::api::openai_responses_shared::ResponsesStreamFailure::Message(message) => Failure::Message(message),
        })?;

    if stream_options.request.signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Err(Failure::Message("Request was aborted".to_owned().into()));
    }
    if matches!(
        output.stop_reason,
        crate::types::StopReason::Aborted | crate::types::StopReason::Error
    ) {
        return Err(Failure::Message(
            output
                .error_message
                .clone()
                .unwrap_or_else(|| "An unknown error occurred".to_owned().into()),
        ));
    }
    push_responses_done(sender, output).map_err(|error| Failure::Message(error.into()))?;
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
        let options = OpenAIResponsesOptions::from_stream_options(&stream_options);
        let mut output = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis() as f64);
        let result = run_stream(&model, &context, &stream_options, &options, &mut output, &sender).await;
        if let Err(failure) = result {
            for block in &mut output.content {
                match block {
                    crate::types::AssistantContent::Text(block) => block.index = None,
                    crate::types::AssistantContent::Thinking(block) => block.index = None,
                    crate::types::AssistantContent::ToolCall(block) => {
                        block.index = None;
                        block.partial_json = None;
                    }
                }
            }
            output.stop_reason = if stream_options.request.signal.as_ref().is_some_and(AbortSignal::aborted) {
                crate::types::StopReason::Aborted
            } else {
                crate::types::StopReason::Error
            };
            output.error_message = Some(failure.format());
            sender.push(crate::types::AssistantMessageEvent::Error {
                reason: if output.stop_reason == crate::types::StopReason::Aborted {
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

/// `stream`.
pub fn stream(
    model: Model,
    context: Context,
    options: StreamOptions,
) -> crate::utils::event_stream::AssistantMessageEventStream {
    run_and_emit(&model, &context, options)
}

/// `streamSimple`.
pub fn stream_simple(
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> Result<crate::utils::event_stream::AssistantMessageEventStream, CallbackError> {
    let api_key = match get_client_api_key(
        &model.provider,
        options.stream.request.api_key.as_ref(),
        options.stream.request.headers.as_ref(),
    ) {
        Ok(api_key) => api_key,
        Err(message) => return Err(CallbackError::new(message)),
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
    Ok(run_and_emit(&model, &context, stream_options))
}

/// `ProviderStreams` adapter for `openAIResponsesApi()`.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenAIResponsesApi;

impl ProviderStreams for OpenAIResponsesApi {
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
        stream_simple(model, context, options)
    }
}

/// `openAIResponsesApi()` from `openai-responses.lazy.ts`.
pub fn open_ai_responses_api() -> LazyApi {
    lazy_api(Arc::new(|| {
        Box::pin(async { Ok(Arc::new(OpenAIResponsesApi) as Arc<dyn ProviderStreams>) })
            as BoxFuture<'static, Result<Arc<dyn ProviderStreams>, CallbackError>>
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_tier_multiplier_matches_provider_rules() {
        assert_eq!(get_service_tier_cost_multiplier("gpt-5.5", Some("flex")), 0.5);
        assert_eq!(get_service_tier_cost_multiplier("gpt-5.5", Some("priority")), 2.5);
        assert_eq!(get_service_tier_cost_multiplier("gpt-5.4", Some("priority")), 2.0);
        assert_eq!(get_service_tier_cost_multiplier("gpt-5.4", None), 1.0);
    }
}
