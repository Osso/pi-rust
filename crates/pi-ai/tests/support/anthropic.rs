//! Request/option helpers and event normalization for the Anthropic Messages
//! adapter tests. HTTP scripting reuses the shared `RecordingFetch`.

use crate::shared_recording_fetch as recording_fetch;

pub use recording_fetch::{RecordingFetch, ScriptedResponse};

use std::sync::Arc;

use indexmap::IndexMap;
use pi_ai::request::{Fetch, HttpRequest, ProviderRequestOptions, StreamOptions};
use pi_ai::types::{
    AssistantMessage, AssistantMessageEvent, CacheRetention, JsonObject, ProviderEnv, ProviderHeaders, Transport,
};
use pi_ai::utils::event_stream::AssistantMessageEventStream;

use pi_ai::utils::headers::headers_to_record;
use pi_js_compat::abort::AbortSignal;
use serde_json::{Map, Value};

pub fn json_options(options: &Value) -> JsonObject {
    options.as_object().cloned().unwrap_or_default()
}

fn headers_from_json(value: Option<&Value>) -> Option<ProviderHeaders> {
    let object = value?.as_object()?;
    Some(
        object
            .iter()
            .map(|(name, value)| (name.clone(), value.as_str().map(pi_js_compat::js_json::JsString::from)))
            .collect(),
    )
}

fn env_from_json(value: Option<&Value>) -> Option<ProviderEnv> {
    let object = value?.as_object()?;
    Some(
        object
            .iter()
            .filter_map(|(name, value)| {
                value
                    .as_str()
                    .map(|value| (name.clone(), pi_js_compat::js_json::JsString::from(value)))
            })
            .collect(),
    )
}

fn parse<T: serde::de::DeserializeOwned>(value: Option<&Value>) -> Option<T> {
    value.and_then(|value| serde_json::from_value(value.clone()).ok())
}

const STREAM_KEYS: [&str; 18] = [
    "apiKey",
    "headers",
    "env",
    "timeoutMs",
    "maxRetries",
    "maxRetryDelayMs",
    "temperature",
    "maxTokens",
    "transport",
    "cacheRetention",
    "sessionId",
    "metadata",
    "signal",
    "onRetry",
    "websocketConnectTimeoutMs",
    "onPayload",
    "onResponse",
    "transformHeaders",
];

pub fn stream_options(options: &Value, fetch: Arc<dyn Fetch>, signal: Option<AbortSignal>) -> StreamOptions {
    let mut extra = json_options(options);
    for key in STREAM_KEYS {
        extra.remove(key);
    }
    StreamOptions {
        request: ProviderRequestOptions {
            signal,
            api_key: (options["apiKey"].as_str().map(str::to_owned)).map(pi_js_compat::js_json::JsString::from),
            fetch: Some(fetch),
            env: env_from_json(options.get("env")),
            headers: headers_from_json(options.get("headers")),
            timeout_ms: options["timeoutMs"].as_f64(),
            max_retries: options["maxRetries"].as_f64(),
            max_retry_delay_ms: options["maxRetryDelayMs"].as_f64(),
            ..ProviderRequestOptions::default()
        },
        temperature: options["temperature"].as_f64(),
        max_tokens: options["maxTokens"].as_f64(),
        transport: parse::<Transport>(options.get("transport")),
        cache_retention: parse::<CacheRetention>(options.get("cacheRetention")),
        session_id: (options["sessionId"].as_str().map(str::to_owned)).map(pi_js_compat::js_json::JsString::from),
        websocket_connect_timeout_ms: options["websocketConnectTimeoutMs"].as_f64(),
        metadata: options
            .get("metadata")
            .and_then(|value| pi_js_compat::js_json::JsValue::from(value.clone()).as_object().cloned()),
        extra,
        ..StreamOptions::default()
    }
}

pub async fn collect(mut stream: AssistantMessageEventStream) -> (Vec<AssistantMessageEvent>, AssistantMessage) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("stream must not fail"));
    }
    let result = stream.result().await.expect("stream result");
    (events, result)
}

pub fn normalize_message(message: &mut Value) {
    let Some(object) = message.as_object_mut() else {
        return;
    };
    object.insert("timestamp".to_owned(), Value::from(0));
}

pub fn normalize_event(event: &mut Value) {
    let Some(object) = event.as_object_mut() else {
        return;
    };
    for key in ["partial", "message", "error"] {
        if let Some(message) = object.get_mut(key) {
            normalize_message(message);
        }
    }
}

pub fn events_json(events: &[AssistantMessageEvent]) -> Vec<Value> {
    events
        .iter()
        .map(|event| {
            let mut value = serde_json::to_value(event).expect("event");
            normalize_event(&mut value);
            value
        })
        .collect()
}

pub fn message_json(message: &AssistantMessage) -> Value {
    let mut value = serde_json::to_value(message).expect("message");
    normalize_message(&mut value);
    value
}

pub fn requests_json(requests: &[HttpRequest]) -> Vec<Value> {
    requests
        .iter()
        .map(|request| {
            let headers: IndexMap<String, String> = headers_to_record(request.headers.iter().cloned());
            let body = request
                .body
                .as_ref()
                .map(|body| String::from_utf8_lossy(body).into_owned());
            let mut object = Map::new();
            object.insert("method".to_owned(), Value::String(request.method.clone()));
            object.insert("url".to_owned(), Value::String(request.url.clone()));
            object.insert("headers".to_owned(), serde_json::to_value(headers).expect("headers"));
            object.insert("body".to_owned(), body.map(Value::String).unwrap_or(Value::Null));
            Value::Object(object)
        })
        .collect()
}

/// SDK identity headers depend on the host runtime (Node version, platform).
/// Pin them through `model.headers` so the recorded request is deterministic.
pub fn sdk_environment_headers() -> Value {
    serde_json::json!({
        "X-Stainless-OS": "Linux",
        "X-Stainless-Arch": "x64",
        "X-Stainless-Runtime": "node",
        "X-Stainless-Runtime-Version": "v22.0.0",
    })
}
