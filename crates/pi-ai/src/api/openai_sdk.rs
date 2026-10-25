//! Private emulation of the observable `openai@6.26.0` JavaScript SDK behavior:
//! URL/header construction, JSON body encoding, HTTP error mapping, and SSE
//! decoding. All provider modules send through [`SdkClient::create_stream`].

use pi_js_compat::js_json::{JsString, JsValue};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use indexmap::IndexMap;
use pi_js_compat::abort::AbortSignal;
use serde_json::{Map, Value};

use crate::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use crate::utils::provider_retry::{ProviderError, ProviderErrorHeaders, ProviderRequestError};

/// Pinned SDK version whose headers and SSE behavior are reproduced.
pub const OPENAI_JS_VERSION: &str = "6.26.0";

/// Azure overrides: `api-key` auth, `api-version` query, deployment path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureClientConfig {
    pub api_version: JsString,
}

/// Client options passed to the SDK stand-in.
#[derive(Clone)]
pub struct SdkClientConfig {
    pub api_key: JsString,
    pub base_url: JsString,
    /// Merged model/options headers; `None` suppresses a default header.
    pub default_headers: IndexMap<String, Option<JsString>>,
    pub timeout_ms: Option<f64>,
    pub fetch: Option<Arc<dyn Fetch>>,
    pub azure: Option<AzureClientConfig>,
}

#[derive(Clone)]
pub struct SdkClient {
    config: SdkClientConfig,
}

/// A successful streaming response plus the SDK-decoded event source.
pub struct SdkResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub stream: SseJsonStream,
}

/// A successful non-streaming response (`create(..., { stream: false })`).
pub struct SdkJsonResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub data: JsValue,
}

#[derive(Debug)]
pub enum SdkError {
    /// SDK/API error carrying the fields `normalizeProviderError` probes.
    Provider(Box<ProviderError>),
    /// Non-`Error` failure text (JSON syntax errors, connection text).
    Message(JsString),
}

impl SdkClient {
    pub fn new(config: SdkClientConfig) -> Self {
        Self { config }
    }

    pub fn for_retry(&self, retry_count: usize) -> Self {
        let mut client = self.clone();
        let overridden = client
            .config
            .default_headers
            .keys()
            .any(|name| name.eq_ignore_ascii_case("x-stainless-retry-count"));
        if !overridden {
            client.config.default_headers.insert(
                "x-stainless-retry-count".to_owned(),
                Some(retry_count.to_string().into()),
            );
        }
        client
    }

    /// `client.chat.completions.create(params, { stream: true }).withResponse()`.
    pub fn create_chat_completion_stream(
        &self,
        params: JsValue,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<SdkResponse, ProviderRequestError<CallbackError>>> {
        self.create_stream("/chat/completions", params, signal)
    }

    /// `client.chat.completions.create(params, { stream: false }).withResponse()`.
    pub fn create_chat_completion(
        &self,
        params: JsValue,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<SdkJsonResponse, ProviderRequestError<CallbackError>>> {
        Box::pin(async move { self.send_json("/chat/completions", params, signal).await })
    }

    /// `client.responses.create(params, { stream: true }).withResponse()`.
    pub fn create_response_stream(
        &self,
        params: JsValue,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<SdkResponse, ProviderRequestError<CallbackError>>> {
        // The pinned Azure SDK does not list `/responses` as a deployment
        // endpoint, so both clients call `/responses` directly and send the
        // deployment name as `body.model`.
        self.create_stream("/responses", params, signal)
    }

    fn create_stream(
        &self,
        path: &str,
        params: JsValue,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<SdkResponse, ProviderRequestError<CallbackError>>> {
        let path = path.to_owned();
        Box::pin(async move { self.send(&path, params, signal).await })
    }

    async fn send(
        &self,
        path: &str,
        params: JsValue,
        signal: Option<AbortSignal>,
    ) -> Result<SdkResponse, ProviderRequestError<CallbackError>> {
        let response = self.send_raw(path, &params, signal.clone()).await?;
        let headers = response.headers.clone();
        Ok(SdkResponse {
            status: response.status,
            headers: response.headers,
            stream: SseJsonStream::new(response.body, signal, headers),
        })
    }

    async fn send_json(
        &self,
        path: &str,
        params: JsValue,
        signal: Option<AbortSignal>,
    ) -> Result<SdkJsonResponse, ProviderRequestError<CallbackError>> {
        let response = self.send_raw(path, &params, signal.clone()).await?;
        let status = response.status;
        let headers = response.headers.clone();
        let text = String::from_utf8_lossy(&read_success_body(response.body, signal.as_ref()).await?).into_owned();
        let json_content = headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-type") && (value.contains("application/json") || value.contains("+json"))
        });
        let data = if status == 204 {
            JsValue::Null
        } else if json_content {
            pi_js_compat::js_json::parse_lossless(&text)
                .map_err(|error| ProviderRequestError::Other(CallbackError::new(error.into_message())))?
        } else {
            JsValue::String(text.into())
        };
        Ok(SdkJsonResponse { status, headers, data })
    }

    async fn send_raw(
        &self,
        path: &str,
        params: &JsValue,
        signal: Option<AbortSignal>,
    ) -> Result<HttpResponse, ProviderRequestError<CallbackError>> {
        if signal.as_ref().is_some_and(AbortSignal::aborted) {
            return Err(ProviderRequestError::Abort(crate::utils::abort::AbortError::new()));
        }
        let body = serde_json::to_vec(params)
            .map_err(|error| ProviderRequestError::Other(CallbackError::new(error.to_string())))?;
        let request = HttpRequest {
            method: "POST".to_owned(),
            url: self.build_url(path).map_err(ProviderRequestError::Other)?,
            headers: self.build_headers().map_err(ProviderRequestError::Other)?,
            body: Some(body),
        };
        let fetch = self.config.fetch.clone().unwrap_or_else(crate::http::default_fetch);
        let timeout = self
            .config
            .timeout_ms
            .unwrap_or(super::stainless_fetch::DEFAULT_TIMEOUT_MS);
        let response = match super::stainless_fetch::fetch_with_timeout(fetch, request, signal.as_ref(), timeout).await
        {
            Ok(response) => response,
            Err(_) if signal.as_ref().is_some_and(AbortSignal::aborted) => {
                return Err(ProviderRequestError::Abort(crate::utils::abort::AbortError::new()));
            }
            Err(error) => {
                let timeout =
                    super::stainless_fetch::is_connection_timeout(&error).map_err(ProviderRequestError::Other)?;
                return Err(ProviderRequestError::Provider(ProviderError {
                    message: if timeout {
                        "Request timed out."
                    } else {
                        "Connection error."
                    }
                    .into(),
                    status: None,
                    headers: ProviderErrorHeaders::new(),
                    body: None,
                    inner_error: None,
                }));
            }
        };

        if !(200..300).contains(&response.status) {
            return Err(ProviderRequestError::Provider(read_http_error(response, signal).await));
        }
        Ok(response)
    }

    fn build_url(&self, path: &str) -> Result<String, CallbackError> {
        let joined = if self.config.base_url.ends_with('/') && path.starts_with('/') {
            pi_js_compat::js_format!("{}{}", self.config.base_url, &path[1..])
        } else {
            pi_js_compat::js_format!("{}{}", self.config.base_url, path)
        };
        let serialized = crate::request::request_url(&joined)?;
        let mut url = reqwest::Url::parse(&serialized).expect("request_url already parsed URL");
        let mut query = url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        if let Some(azure) = &self.config.azure {
            query.push((
                "api-version".to_owned(),
                pi_js_compat::js_json::encode_node_utf8(&azure.api_version),
            ));
        }
        if !query.is_empty() {
            url.query_pairs_mut().clear().extend_pairs(query);
        }
        Ok(url.to_string())
    }

    fn build_headers(&self) -> Result<Vec<(String, String)>, CallbackError> {
        let mut headers: Vec<(String, JsString)> = Vec::new();
        set_header(&mut headers, "accept", "application/json");
        let client_name = if self.config.azure.is_some() {
            "AzureOpenAI"
        } else {
            "OpenAI"
        };
        set_header(
            &mut headers,
            "user-agent",
            format!("{client_name}/JS {OPENAI_JS_VERSION}"),
        );
        set_header(&mut headers, "x-stainless-retry-count", "0");
        if let Some(timeout_ms) = self.config.timeout_ms {
            set_header(
                &mut headers,
                "x-stainless-timeout",
                format!("{}", (timeout_ms / 1000.0).trunc()),
            );
        }
        set_header(&mut headers, "x-stainless-lang", "js");
        set_header(&mut headers, "x-stainless-package-version", OPENAI_JS_VERSION);
        set_header(&mut headers, "x-stainless-os", platform_os());
        set_header(&mut headers, "x-stainless-arch", platform_arch());
        set_header(&mut headers, "x-stainless-runtime", "node");
        set_header(&mut headers, "x-stainless-runtime-version", "unknown");
        if self.config.azure.is_some() {
            set_header(&mut headers, "api-key", &self.config.api_key);
        } else {
            set_header(
                &mut headers,
                "authorization",
                pi_js_compat::js_format!("Bearer {}", self.config.api_key),
            );
        }
        for (name, value) in &self.config.default_headers {
            match value {
                Some(value) => set_header(&mut headers, name, value),
                None => remove_header(&mut headers, name),
            }
        }
        set_header(&mut headers, "content-type", "application/json");
        headers
            .into_iter()
            .map(|(name, value)| crate::request::header_value(&value).map(|value| (name, value)))
            .collect()
    }
}

fn set_header(headers: &mut Vec<(String, JsString)>, name: &str, value: impl Into<JsString>) {
    let value = value.into();
    let lower_name = name.to_lowercase();
    if let Some(existing) = headers
        .iter_mut()
        .find(|(existing, _)| existing.eq_ignore_ascii_case(&lower_name))
    {
        existing.1 = value.clone();
        return;
    }
    headers.push((lower_name, value));
}

fn remove_header(headers: &mut Vec<(String, JsString)>, name: &str) {
    headers.retain(|(existing, _)| !existing.eq_ignore_ascii_case(name));
}

fn platform_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "macos") {
        "MacOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "freebsd") {
        "FreeBSD"
    } else if cfg!(target_os = "openbsd") {
        "OpenBSD"
    } else {
        "Unknown"
    }
}

fn platform_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x32"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else {
        "unknown"
    }
}

async fn read_success_body(
    mut body: Box<dyn HttpBody>,
    signal: Option<&AbortSignal>,
) -> Result<Vec<u8>, ProviderRequestError<CallbackError>> {
    let mut bytes = Vec::new();
    loop {
        if signal.is_some_and(AbortSignal::aborted) {
            return Err(ProviderRequestError::Abort(crate::utils::abort::AbortError::new()));
        }
        match body.next_chunk().await {
            Some(Ok(chunk)) => bytes.extend_from_slice(&chunk),
            Some(Err(_)) if signal.is_some_and(AbortSignal::aborted) => {
                return Err(ProviderRequestError::Abort(crate::utils::abort::AbortError::new()));
            }
            Some(Err(error)) => return Err(ProviderRequestError::Other(error)),
            None => return Ok(bytes),
        }
    }
}

async fn read_http_error(response: HttpResponse, signal: Option<AbortSignal>) -> ProviderError {
    let mut body = response.body;
    let mut bytes = Vec::new();
    loop {
        if signal.as_ref().is_some_and(AbortSignal::aborted) {
            break;
        }
        match body.next_chunk().await {
            Some(Ok(chunk)) => bytes.extend_from_slice(&chunk),
            Some(Err(_)) | None => break,
        }
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let parsed = pi_js_compat::js_json::parse_lossless(text.as_str());
    let inner_error = match &parsed {
        Ok(value) => value.get("error").cloned(),
        Err(_) => None,
    };
    let message = error_message(response.status, inner_error.as_ref(), &text, parsed.is_ok());
    let body = inner_error
        .as_ref()
        .filter(|error| is_plain_non_empty_object(error))
        .map(|error| pi_js_compat::js_json::stringify_lossless(error).into());
    let mut headers = ProviderErrorHeaders::new();
    for (name, value) in &response.headers {
        headers.insert(name, value);
    }
    ProviderError {
        message,
        status: Some(f64::from(response.status)),
        headers,
        body,
        inner_error: inner_error.map(Box::new),
    }
}

fn is_plain_non_empty_object(value: &JsValue) -> bool {
    matches!(value, JsValue::Object(object) if !object.is_empty())
}

fn error_message(status: u16, inner_error: Option<&JsValue>, raw_text: &str, parsed: bool) -> JsString {
    let message = match inner_error {
        Some(error) => Some(api_error_detail(error)),
        None if parsed => None,
        None => Some(JsString::from(raw_text)),
    };
    match message {
        Some(message) if !message.is_empty() => JsString::join([format!("{status} ").into(), message], ""),
        _ => format!("{status} status code (no body)").into(),
    }
}

fn api_error_detail(error: &JsValue) -> JsString {
    match error.get("message") {
        Some(JsValue::String(message)) => message.clone(),
        Some(message) => pi_js_compat::js_json::stringify_lossless(message).into(),
        None => pi_js_compat::js_json::stringify_lossless(error).into(),
    }
}

/// Decoded SSE events plus the SDK error mapping.
pub struct SseJsonStream {
    body: Box<dyn HttpBody>,
    buffer: Vec<u8>,
    eof: bool,
    done: bool,
    signal: Option<AbortSignal>,
    headers: BTreeMap<String, String>,
    pending: VecDeque<SseEvent>,
}

struct SseEvent {
    data: String,
}

impl SseJsonStream {
    fn new(body: Box<dyn HttpBody>, signal: Option<AbortSignal>, headers: Vec<(String, String)>) -> Self {
        Self {
            body,
            buffer: Vec::new(),
            eof: false,
            done: false,
            signal,
            headers: headers.into_iter().collect(),
            pending: VecDeque::new(),
        }
    }

    pub async fn next_event(&mut self) -> Result<Option<JsValue>, SdkError> {
        if self.done {
            return Ok(None);
        }
        if self.pending.is_empty() {
            self.fill_events().await?;
            if self.pending.is_empty() {
                return Ok(None);
            }
        }
        let event = self.pending.pop_front().expect("event is queued");
        if event.data.starts_with("[DONE]") {
            self.done = true;
            return Ok(None);
        }
        let value: JsValue = pi_js_compat::js_json::parse_lossless(&event.data)
            .map_err(|error| SdkError::Message(error.into_message()))?;
        if let Some(error) = value.get("error").filter(|error| is_truthy_ingress(error)) {
            let message = api_error_detail(error);
            let mut headers = ProviderErrorHeaders::new();
            for (name, value) in &self.headers {
                headers.insert(name, value);
            }
            return Err(SdkError::Provider(Box::new(ProviderError {
                message,
                status: None,
                headers,
                body: is_plain_non_empty_object(error).then(|| pi_js_compat::js_json::stringify_lossless(error).into()),
                inner_error: Some(Box::new(error.clone())),
            })));
        }
        Ok(Some(value))
    }

    async fn fill_events(&mut self) -> Result<(), SdkError> {
        loop {
            if let Some(segment) = take_sse_segment(&mut self.buffer) {
                self.pending
                    .extend(decode_sse_segment(&String::from_utf8_lossy(&segment)));
                if !self.pending.is_empty() {
                    return Ok(());
                }
                continue;
            }
            if self.eof {
                if self.buffer.is_empty() {
                    return Ok(());
                }
                let remaining = std::mem::take(&mut self.buffer);
                self.pending
                    .extend(decode_sse_segment(&String::from_utf8_lossy(&remaining)));
                return Ok(());
            }
            if self.signal.as_ref().is_some_and(AbortSignal::aborted) {
                self.eof = true;
                continue;
            }
            match self.body.next_chunk().await {
                Some(Ok(chunk)) => self.buffer.extend_from_slice(&chunk),
                Some(Err(_)) if self.signal.as_ref().is_some_and(AbortSignal::aborted) => {
                    // The SDK swallows abort errors while iterating a stream.
                    self.eof = true;
                }
                Some(Err(error)) => return Err(SdkError::Message(error.message)),
                None => self.eof = true,
            }
        }
    }
}

fn take_sse_segment(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let index = find_double_newline_index(buffer)?;
    let segment = buffer[..index].to_vec();
    buffer.drain(..index);
    Some(segment)
}

fn find_double_newline_index(buffer: &[u8]) -> Option<usize> {
    for index in 0..buffer.len().saturating_sub(1) {
        if buffer[index] == b'\n' && buffer[index + 1] == b'\n' {
            return Some(index + 2);
        }
        if buffer[index] == b'\r' && buffer[index + 1] == b'\r' {
            return Some(index + 2);
        }
        if buffer[index] == b'\r'
            && buffer[index + 1] == b'\n'
            && index + 3 < buffer.len()
            && buffer[index + 2] == b'\r'
            && buffer[index + 3] == b'\n'
        {
            return Some(index + 4);
        }
    }
    None
}

fn decode_sse_segment(segment: &str) -> Vec<SseEvent> {
    let mut events = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let normalized = segment.replace("\r\n", "\n").replace('\r', "\n");
    for line in normalized.split('\n') {
        if line.is_empty() {
            if data.is_empty() {
                continue;
            }
            events.push(SseEvent { data: data.join("\n") });
            data.clear();
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line, ""),
        };
        if field == "data" {
            data.push(value.to_owned());
        }
    }
    events
}

/// Check only named scalar metadata fields, never content or argument subtrees.
pub(crate) fn validate_metadata_fields(value: &JsValue, fields: &[&str]) -> Result<(), String> {
    for field in fields {
        if let Some(text) = value.get(field).and_then(JsValue::as_string) {
            text.to_utf8()
                .map_err(|error| format!("Provider metadata {field}: {error}"))?;
        }
    }
    Ok(())
}

pub(crate) fn validate_chat_metadata(chunk: &JsValue) -> Result<(), String> {
    validate_metadata_fields(chunk, &["id", "model"])?;
    let Some(choice) = chunk
        .get("choices")
        .and_then(JsValue::as_array)
        .and_then(|choices| choices.first())
    else {
        return Ok(());
    };
    validate_metadata_fields(choice, &["finish_reason"])?;
    let Some(delta) = choice.get("delta") else {
        return Ok(());
    };
    if let Some(calls) = delta.get("tool_calls").and_then(JsValue::as_array) {
        for call in calls {
            validate_metadata_fields(call, &["id"])?;
            for kind in ["function", "custom"] {
                if let Some(function) = call.get(kind) {
                    validate_metadata_fields(function, &["name"])?;
                }
            }
        }
    }
    if let Some(details) = delta.get("reasoning_details").and_then(JsValue::as_array) {
        for detail in details {
            validate_metadata_fields(detail, &["type", "id", "data"])?;
        }
    }
    Ok(())
}

/// Call only after the event's schema-specific metadata validation.
pub(crate) fn ingress_metadata_text(value: &JsValue) -> Option<String> {
    value
        .as_string()
        .map(|text| text.to_utf8().expect("provider metadata was checked"))
}

fn is_truthy_ingress(value: &JsValue) -> bool {
    match value {
        JsValue::Null => false,
        JsValue::Bool(value) => *value,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(text) => !text.is_empty(),
        JsValue::Array(_) | JsValue::Object(_) => true,
    }
}

impl crate::api::openai_responses_shared::ResponsesEventSource for SseJsonStream {
    type Error = SdkError;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<JsValue>, Self::Error>> {
        Box::pin(SseJsonStream::next_event(self))
    }
}

/// Format an SDK failure the way the provider modules do: SDK errors go through
/// `normalizeProviderError` + `formatProviderError`, other messages verbatim.
pub fn format_sdk_error(error: &SdkError, prefix: Option<&str>) -> JsString {
    match error {
        SdkError::Provider(error) => {
            let normalized = normalized_provider_error(error);
            crate::utils::error_body::format_provider_error(&normalized, prefix)
        }
        SdkError::Message(message) => message.clone(),
    }
}

/// Format a retry-layer failure (`ProviderRequestError<CallbackError>`).
pub fn format_request_error(error: &ProviderRequestError<CallbackError>, prefix: Option<&str>) -> JsString {
    match error {
        ProviderRequestError::Provider(error) => {
            let normalized = normalized_provider_error(error);
            crate::utils::error_body::format_provider_error(&normalized, prefix)
        }
        ProviderRequestError::Other(error) => error.message.clone(),
        ProviderRequestError::Abort(_) => "Request was aborted.".into(),
        ProviderRequestError::RetryDelayLimit(error) => error.message(),
    }
}

fn normalized_provider_error(error: &ProviderError) -> crate::utils::error_body::NormalizedProviderError {
    let body = error.body.as_ref().map(|body| {
        crate::utils::error_body::truncate_error_text(
            &body.trim(),
            crate::utils::error_body::MAX_PROVIDER_ERROR_BODY_CHARS,
        )
    });
    let body = body.filter(|body| !body.is_empty());
    crate::utils::error_body::NormalizedProviderError {
        status: error.status,
        message_carries_body: body.as_ref().is_none_or(|body| error.message.contains(body)),
        body,
        message: error.message.clone(),
    }
}

/// Inner `error` object from an SDK HTTP error, used for OpenRouter `metadata.raw`.
pub fn sdk_error_inner(error: &SdkError) -> Option<&JsValue> {
    match error {
        SdkError::Provider(error) => error.inner_error.as_deref(),
        SdkError::Message(_) => None,
    }
}

/// Parse a raw JSON object body into an SDK-shaped error (used for tests).
pub fn provider_error_from_body(status: u16, body: &str) -> ProviderError {
    let parsed = pi_js_compat::js_json::parse_lossless(body);
    let inner = match &parsed {
        Ok(value) => value.get("error").cloned(),
        Err(_) => None,
    };
    ProviderError {
        message: error_message(status, inner.as_ref(), body, parsed.is_ok()),
        status: Some(f64::from(status)),
        headers: ProviderErrorHeaders::new(),
        body: inner
            .as_ref()
            .filter(|error| is_plain_non_empty_object(error))
            .map(|error| pi_js_compat::js_json::stringify_lossless(error).into()),
        inner_error: inner.map(Box::new),
    }
}

/// Encode a JSON value the way `JSON.stringify` does for request bodies.
pub fn encode_body(params: &Value) -> Result<Vec<u8>, CallbackError> {
    serde_json::to_vec(params).map_err(|error| CallbackError::new(error.to_string()))
}

/// Build a JSON object, preserving insertion order.
pub fn json_object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key.to_owned(), value);
    }
    Value::Object(map)
}
