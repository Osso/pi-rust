//! Port of `packages/ai/src/api/openrouter-images.ts`.

use indexmap::IndexMap;
use pi_js_compat::js_json::{JsString, JsValue};

use pi_js_compat::abort::AbortSignal;
use serde_json::{Map, Value};

use crate::api::openai_sdk::{SdkClient, SdkClientConfig, SdkJsonResponse, format_request_error};
use crate::request::{BoxFuture, CallbackError, ImagesOptions, ProviderImages, now_millis};
use crate::types::{
    AssistantImages, ImageContent, ImagesContext, ImagesModel, ImagesStopReason, Modality, ProviderResponse,
    TextContent, TextOrImageContent, Usage, UsageCost,
};
use crate::utils::headers::headers_to_record;
use crate::utils::provider_retry::{ProviderRetryOptions, retry_provider_request};
use crate::utils::sanitize_unicode::sanitize_surrogates;

/// `generateImages` for the OpenRouter image API.
///
/// TypeScript encodes every failure in the returned `AssistantImages`; the Rust
/// `ProviderImages` contract is `Result`, and this function always returns `Ok`
/// with `stopReason: "error" | "aborted"` for failures.
pub fn generate_images(
    model: ImagesModel,
    context: ImagesContext,
    options: ImagesOptions,
) -> BoxFuture<'static, Result<AssistantImages, CallbackError>> {
    Box::pin(async move { Ok(run_generate_images(model, context, options).await) })
}

/// `openrouter-images` API implementation for [`ProviderImages`].
pub struct OpenRouterImagesApi;

impl ProviderImages for OpenRouterImagesApi {
    fn generate_images(
        &self,
        model: ImagesModel,
        context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'static, Result<AssistantImages, CallbackError>> {
        generate_images(model, context, options)
    }
}

async fn run_generate_images(model: ImagesModel, context: ImagesContext, options: ImagesOptions) -> AssistantImages {
    let mut output = AssistantImages {
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        output: Vec::new(),
        response_id: None,
        usage: None,
        stop_reason: ImagesStopReason::Stop,
        error_message: None,
        timestamp: now_millis() as f64,
    };

    match request_images(&model, &context, &options).await {
        Ok((data, response)) => {
            if let Some(on_response) = &options.request.on_response
                && let Err(error) = on_response(response, model.clone()).await
            {
                return fail(&mut output, &options, error.error_or_json_message());
            }
            match apply_response(&mut output, &data, &model) {
                Ok(()) => output,
                Err(message) => fail(&mut output, &options, message),
            }
        }
        Err(message) => fail(&mut output, &options, message),
    }
}

fn fail(
    output: &mut AssistantImages,
    options: &ImagesOptions,
    message: pi_js_compat::js_json::JsString,
) -> AssistantImages {
    output.stop_reason = if options.request.signal.as_ref().is_some_and(AbortSignal::aborted) {
        ImagesStopReason::Aborted
    } else {
        ImagesStopReason::Error
    };
    output.error_message = Some(message);
    output.clone()
}

async fn request_images(
    model: &ImagesModel,
    context: &ImagesContext,
    options: &ImagesOptions,
) -> Result<(JsValue, ProviderResponse), pi_js_compat::js_json::JsString> {
    let api_key = options
        .request
        .api_key
        .as_ref()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| format!("No API key for provider: {}", model.provider))?;
    let params = build_effective_params(model, context, options).await?;
    let client = build_client(model, api_key, options);
    let response = send_with_retries(&client, params, options).await?;
    Ok(provider_response(response))
}

async fn build_effective_params(
    model: &ImagesModel,
    context: &ImagesContext,
    options: &ImagesOptions,
) -> Result<JsValue, pi_js_compat::js_json::JsString> {
    let params = JsValue::from(build_params(model, context));
    let Some(on_payload) = &options.request.on_payload else {
        return Ok(params);
    };
    let next_params = on_payload(params.clone(), model.clone())
        .await
        .map_err(|error| error.error_or_json_message())?;
    Ok(next_params.unwrap_or(params))
}

fn build_client(model: &ImagesModel, api_key: &JsString, options: &ImagesOptions) -> SdkClient {
    SdkClient::new(SdkClientConfig {
        api_key: api_key.to_owned(),
        base_url: model.base_url.clone(),
        default_headers: merged_headers(model, options),
        timeout_ms: options.request.timeout_ms,
        fetch: options.request.fetch.clone(),
        azure: None,
    })
}

async fn send_with_retries(
    client: &SdkClient,
    params: JsValue,
    options: &ImagesOptions,
) -> Result<SdkJsonResponse, pi_js_compat::js_json::JsString> {
    let signal = options.request.signal.clone();
    let retry_options = ProviderRetryOptions {
        max_retries: options
            .request
            .max_retries
            .map_or(0, |retries| retries.max(0.0) as usize),
        // The SDK request path does not consume pi's maxRetryDelayMs option.
        max_retry_delay_ms: Some(0.0),
        signal: signal.clone(),
    };
    let mut attempt = 0;
    retry_provider_request(
        || {
            let client = client.for_retry(attempt);
            attempt += 1;
            let params = params.clone();
            let signal = signal.clone();
            async move { client.create_chat_completion(params, signal).await }
        },
        retry_options,
    )
    .await
    .map_err(|error| format_request_error(&error, None))
}

fn provider_response(response: SdkJsonResponse) -> (JsValue, ProviderResponse) {
    (
        response.data,
        ProviderResponse {
            status: f64::from(response.status),
            headers: headers_to_record(response.headers),
        },
    )
}

fn merged_headers(model: &ImagesModel, options: &ImagesOptions) -> IndexMap<String, Option<JsString>> {
    let mut merged: IndexMap<String, Option<JsString>> = IndexMap::new();
    if let Some(headers) = &model.headers {
        for (name, value) in headers {
            merged.insert(name.clone(), Some(value.clone()));
        }
    }
    if let Some(headers) = &options.request.headers {
        for (name, value) in headers {
            merged.insert(name.clone(), value.clone());
        }
    }
    // `providerHeadersToRecord` drops explicit suppressions before the SDK sees them.
    merged.retain(|_, value| value.is_some());
    merged
}

fn build_params(model: &ImagesModel, context: &ImagesContext) -> Value {
    let content: Vec<Value> = context.input.iter().map(content_part).collect();

    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String("user".to_owned()));
    message.insert("content".to_owned(), Value::Array(content));

    let modalities = request_modalities(model);

    let mut params = Map::new();
    params.insert("model".to_owned(), Value::String(model.id.clone()));
    params.insert("messages".to_owned(), Value::Array(vec![Value::Object(message)]));
    params.insert("stream".to_owned(), Value::Bool(false));
    params.insert(
        "modalities".to_owned(),
        Value::Array(
            modalities
                .into_iter()
                .map(|modality| Value::String(modality.to_owned()))
                .collect(),
        ),
    );
    Value::Object(params)
}

fn content_part(item: &TextOrImageContent) -> Value {
    match item {
        TextOrImageContent::Text(text) => {
            let mut part = Map::new();
            part.insert("type".to_owned(), Value::String("text".to_owned()));
            part.insert(
                "text".to_owned(),
                Value::String(sanitize_surrogates(&text.text).to_owned()),
            );
            Value::Object(part)
        }
        TextOrImageContent::Image(image) => {
            let mut image_url = Map::new();
            image_url.insert(
                "url".to_owned(),
                Value::String(format!("data:{};base64,{}", image.mime_type, image.data)),
            );
            let mut part = Map::new();
            part.insert("type".to_owned(), Value::String("image_url".to_owned()));
            part.insert("image_url".to_owned(), Value::Object(image_url));
            Value::Object(part)
        }
    }
}

fn request_modalities(model: &ImagesModel) -> Vec<&'static str> {
    if model.output.contains(&Modality::Text) {
        vec!["image", "text"]
    } else {
        vec!["image"]
    }
}

fn apply_response(output: &mut AssistantImages, data: &JsValue, model: &ImagesModel) -> Result<(), JsString> {
    if data.is_null() {
        return Err("Cannot read properties of null (reading 'id')".into());
    }
    super::openai_sdk::validate_metadata_fields(data, &["id"]).map_err(JsString::from)?;
    output.response_id = data
        .get("id")
        .and_then(JsValue::as_string)
        .map(|id| id.to_utf8().expect("response ID validated as opaque scalar metadata"));
    if let Some(usage) = data.get("usage").filter(|usage| !usage.is_null()) {
        output.usage = Some(parse_usage(usage, model));
    }

    let choices = data
        .get("choices")
        .ok_or_else(|| JsString::from("Cannot read properties of undefined (reading '0')"))?;
    let Some(choice) = choices.as_array().and_then(|choices| choices.first()) else {
        return Ok(());
    };
    let message = choice
        .get("message")
        .ok_or_else(|| JsString::from("Cannot read properties of undefined (reading 'content')"))?;
    push_text(output, message);
    push_images(output, message)?;
    Ok(())
}

fn push_text(output: &mut AssistantImages, message: &JsValue) {
    if let Some(content) = message.get("content").and_then(JsValue::as_string)
        && !content.is_empty()
    {
        output.output.push(TextOrImageContent::Text(TextContent::new(content)));
    }
}

fn push_images(output: &mut AssistantImages, message: &JsValue) -> Result<(), JsString> {
    let images = message
        .get("images")
        .and_then(JsValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for image in images {
        let Some(url) = image_url(image) else {
            continue;
        };
        let url = url
            .to_utf8()
            .map_err(|_| JsString::from("OpenRouter image URL contains an unpaired surrogate"))?;
        let Some((mime_type, data)) = parse_data_url(&url) else {
            continue;
        };
        output
            .output
            .push(TextOrImageContent::Image(ImageContent::new(data, mime_type)));
    }
    Ok(())
}

fn image_url(image: &JsValue) -> Option<&JsString> {
    match image.get("image_url") {
        Some(JsValue::String(url)) => Some(url),
        Some(JsValue::Object(object)) => object.get("url").and_then(JsValue::as_string),
        _ => None,
    }
}

fn parse_data_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("data:")?;
    let (mime_type, payload) = rest.split_once(';')?;
    if mime_type.is_empty() {
        return None;
    }
    let data = payload.strip_prefix("base64,")?;
    if data.is_empty() || data.contains(['\n', '\r']) {
        return None;
    }
    Some((mime_type, data))
}

struct TokenCounts {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
}

fn parse_usage(raw: &JsValue, model: &ImagesModel) -> Usage {
    let tokens = token_counts(raw);
    Usage {
        input: tokens.input,
        output: tokens.output,
        cache_read: tokens.cache_read,
        cache_write: tokens.cache_write,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: tokens.input + tokens.output + tokens.cache_read + tokens.cache_write,
        cost: usage_cost(model, &tokens),
    }
}

fn token_counts(raw: &JsValue) -> TokenCounts {
    let number = |name: &str| raw.get(name).and_then(JsValue::as_f64).unwrap_or(0.0);
    let details = raw.get("prompt_tokens_details");
    let detail = |name: &str| {
        details
            .and_then(|details| details.get(name))
            .and_then(JsValue::as_f64)
            .unwrap_or(0.0)
    };

    let prompt_tokens = number("prompt_tokens");
    let reported_cached_tokens = detail("cached_tokens");
    let cache_write = detail("cache_write_tokens");
    let cache_read = if cache_write > 0.0 {
        (reported_cached_tokens - cache_write).max(0.0)
    } else {
        reported_cached_tokens
    };
    TokenCounts {
        input: (prompt_tokens - cache_read - cache_write).max(0.0),
        output: number("completion_tokens"),
        cache_read,
        cache_write,
    }
}

fn usage_cost(model: &ImagesModel, tokens: &TokenCounts) -> UsageCost {
    let rates = &model.cost.rates;
    let input = (rates.input / 1_000_000.0) * tokens.input;
    let output = (rates.output / 1_000_000.0) * tokens.output;
    let cache_read = (rates.cache_read / 1_000_000.0) * tokens.cache_read;
    let cache_write = (rates.cache_write / 1_000_000.0) * tokens.cache_write;
    UsageCost {
        input,
        output,
        cache_read,
        cache_write,
        total: input + output + cache_read + cache_write,
    }
}
