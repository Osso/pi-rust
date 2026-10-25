//! Private stand-in for `@google/genai` and `google-auth-library` used by the
//! Google Generative AI and Google Vertex adapters.
//!
//! Only the observable behavior Pi depends on is implemented: request URL and
//! body construction for `models.generateContentStream`, SSE chunk parsing,
//! `ApiError`-shaped HTTP failures, and ADC service-account/authorized-user
//! access-token exchange.

use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use indexmap::IndexMap;
use pi_js_compat::abort::AbortSignal;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding, Signer};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::api::google_shared::{GoogleRequestError, is_thinking_part, map_stop_reason, retain_thought_signature};
use crate::http::default_fetch;
use crate::models::calculate_cost;
use crate::request::{Fetch, HttpBody, HttpRequest, HttpResponse, header_value};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, DoneReason, Model, ProviderEnv, ProviderHeaders,
    StopReason, TextContent, ThinkingContent, ToolCall, Usage, UsageCost,
};
use crate::utils::event_stream::AssistantMessageEventSender;
use crate::utils::headers::provider_headers_to_record;
use crate::utils::provider_env::get_provider_env_value;

const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const JWT_LIFETIME_SECONDS: i64 = 3600;
const VERTEX_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
// Preserve the frozen SDK language label; Rust has no host Node version.
const SDK_USER_AGENT: &str = "google-genai-sdk/1.52.0 gl-node/unknown";
const GOOGLE_AI_DEFAULT_API_VERSION: &str = "v1beta";
const GOOGLE_AI_DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/";

/// Streamed `GenerateContentResponse` fields the adapters read.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleChunk {
    #[serde(default)]
    pub response_id: Option<String>,
    #[serde(default)]
    pub candidates: Option<Vec<GoogleCandidate>>,
    #[serde(default)]
    pub usage_metadata: Option<GoogleUsageMetadata>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleCandidate {
    #[serde(default)]
    pub content: Option<GoogleResponseContent>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GoogleResponseContent {
    #[serde(default)]
    pub parts: Option<Vec<GooglePart>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GooglePart {
    #[serde(default)]
    pub text: Option<JsString>,
    #[serde(default)]
    pub thought: Option<bool>,
    #[serde(default)]
    pub thought_signature: Option<String>,
    #[serde(default)]
    pub function_call: Option<GoogleFunctionCall>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleFunctionCall {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub args: Option<JsValue>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleUsageMetadata {
    #[serde(default)]
    pub prompt_token_count: Option<f64>,
    #[serde(default)]
    pub cached_content_token_count: Option<f64>,
    #[serde(default)]
    pub candidates_token_count: Option<f64>,
    #[serde(default)]
    pub thoughts_token_count: Option<f64>,
    #[serde(default)]
    pub total_token_count: Option<f64>,
}

/// One ordered, case-insensitive header list with `Headers` append semantics.
#[derive(Debug, Default, Clone)]
struct HeaderList {
    entries: Vec<(String, String)>,
}

impl HeaderList {
    fn from_entries(entries: Vec<(String, JsString)>) -> Result<Self, GoogleRequestError> {
        let mut list = Self::default();
        for (name, value) in entries {
            list.append(&name, &value)?;
        }
        Ok(list)
    }

    /// `Headers.append`: ByteString conversion, then HTTP whitespace trim.
    fn append(&mut self, name: &str, value: &JsString) -> Result<(), GoogleRequestError> {
        let value = header_value(value).map_err(|error| GoogleRequestError::new(error.message, None))?;
        let name = name.to_ascii_lowercase();
        let value = value.trim_matches(|character| matches!(character, ' ' | '\t' | '\n' | '\r'));
        match self.entries.iter_mut().find(|(existing, _)| existing == &name) {
            Some((_, existing)) => {
                existing.push_str(", ");
                existing.push_str(value);
            }
            None => self.entries.push((name, value.to_owned())),
        }
        Ok(())
    }

    fn contains(&self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        self.entries.iter().any(|(existing, _)| existing == &name)
    }

    fn into_pairs(self) -> Vec<(String, String)> {
        self.entries
    }
}

/// Vertex endpoint selection derived from `GoogleGenAI`/`ApiClient`.
#[derive(Debug, Clone)]
struct Endpoint {
    vertexai: bool,
    base_url: JsString,
    api_version: String,
    collection_scope: bool,
    project: Option<JsString>,
    location: Option<JsString>,
}

/// ADC credentials accepted by the adapter.
#[derive(Debug, Clone, PartialEq)]
enum Credentials {
    ServiceAccount {
        client_email: String,
        private_key: String,
    },
    AuthorizedUser {
        client_id: String,
        client_secret: String,
        refresh_token: String,
    },
}

/// HTTP client stand-in shared by both Google adapters.
pub struct GoogleClient {
    api_key: Option<JsString>,
    key_filename: Option<JsString>,
    endpoint: Endpoint,
    headers: Vec<(String, JsString)>,
    token: Mutex<Option<(String, f64)>>,
}

/// Open streaming response; chunks are read with [`Self::next_chunk`].
pub struct GoogleStream {
    body: Box<dyn HttpBody>,
    decoder: Utf8Decoder,
    buffer: String,
    done: bool,
}

impl GoogleClient {
    /// Gemini API client (`new GoogleGenAI({apiKey, httpOptions})`).
    pub fn generative_ai(model: &Model, api_key: &JsString, option_headers: Option<&ProviderHeaders>) -> Self {
        let (base_url, api_version) = if !model.base_url.is_empty() {
            (model.base_url.clone(), String::new())
        } else if let Some(base_url) = read_process_env_value("GOOGLE_GEMINI_BASE_URL") {
            (base_url.into(), GOOGLE_AI_DEFAULT_API_VERSION.to_owned())
        } else {
            (
                GOOGLE_AI_DEFAULT_BASE_URL.into(),
                GOOGLE_AI_DEFAULT_API_VERSION.to_owned(),
            )
        };
        Self {
            api_key: Some(api_key.clone()),
            key_filename: None,
            endpoint: Endpoint {
                vertexai: false,
                base_url,
                api_version,
                collection_scope: false,
                project: None,
                location: None,
            },
            headers: merge_headers(model, option_headers),
            token: Mutex::new(None),
        }
    }

    /// Vertex API-key client (`new GoogleGenAI({vertexai: true, apiKey, apiVersion: "v1", ...})`).
    pub fn vertex_api_key(model: &Model, api_key: &JsString, option_headers: Option<&ProviderHeaders>) -> Self {
        let custom_base_url = resolve_custom_base_url(&model.base_url);
        let (base_url, api_version) = match &custom_base_url {
            Some(base_url) => (
                base_url.clone(),
                if base_url_includes_api_version(base_url) {
                    String::new()
                } else {
                    "v1".to_owned()
                },
            ),
            None => (
                read_process_env_value("GOOGLE_VERTEX_BASE_URL")
                    .map_or_else(|| "https://aiplatform.googleapis.com/".into(), JsString::from),
                "v1".to_owned(),
            ),
        };
        Self {
            api_key: Some(api_key.clone()),
            key_filename: None,
            endpoint: Endpoint {
                vertexai: true,
                base_url,
                api_version,
                collection_scope: custom_base_url.is_some(),
                project: None,
                location: None,
            },
            headers: merge_headers(model, option_headers),
            token: Mutex::new(None),
        }
    }

    /// Vertex ADC client (`new GoogleGenAI({vertexai: true, project, location, googleAuthOptions})`).
    pub fn vertex_adc(
        model: &Model,
        project: &JsString,
        location: &JsString,
        option_headers: Option<&ProviderHeaders>,
        env: Option<&ProviderEnv>,
    ) -> Result<Self, GoogleRequestError> {
        let custom_base_url = resolve_custom_base_url(&model.base_url);
        let (base_url, api_version) = match &custom_base_url {
            Some(base_url) => (
                base_url.clone(),
                if base_url_includes_api_version(base_url) {
                    String::new()
                } else {
                    "v1".to_owned()
                },
            ),
            None => (
                read_process_env_value("GOOGLE_VERTEX_BASE_URL")
                    .map_or_else(|| vertex_default_base_url(location), JsString::from),
                "v1".to_owned(),
            ),
        };
        let key_filename = get_provider_env_value("GOOGLE_APPLICATION_CREDENTIALS", env);
        Ok(Self {
            api_key: None,
            key_filename,
            endpoint: Endpoint {
                vertexai: true,
                base_url,
                api_version,
                collection_scope: custom_base_url.is_some(),
                project: Some(project.clone()),
                location: Some(location.clone()),
            },
            headers: merge_headers(model, option_headers),
            token: Mutex::new(None),
        })
    }

    /// `client.models.generateContentStream(params)`, resolved through `fetch`.
    pub async fn generate_content_stream(
        &self,
        params: &JsValue,
        fetch: &dyn Fetch,
        signal: Option<&AbortSignal>,
    ) -> Result<GoogleStream, GoogleRequestError> {
        let model = params
            .get("model")
            .and_then(JsValue::as_string)
            .map(|text| text.to_utf8())
            .transpose()
            .map_err(|_| GoogleRequestError::new("URI malformed", None))?
            .unwrap_or_default();
        let url = self.request_url(&model)?;
        let body = request_body(self.endpoint.vertexai, params);
        let body = pi_js_compat::js_json::stringify_lossless(&body);
        let request_signal = google_request_signal(signal);
        let headers = self.request_headers(fetch, signal).await?;
        let request = HttpRequest {
            method: "POST".to_owned(),
            url,
            headers: headers.into_pairs(),
            body: Some(body.into_bytes()),
        };
        let response = fetch
            .fetch(request, request_signal)
            .await
            .map_err(|error| GoogleRequestError::new(error.error_or_json_message(), None))?;
        if !(200..300).contains(&response.status) {
            return Err(read_http_error(response).await);
        }
        Ok(GoogleStream {
            body: response.body,
            decoder: Utf8Decoder::default(),
            buffer: String::new(),
            done: false,
        })
    }

    /// The SDK hands the URL string to `new URL()`, a USVString: the Node
    /// UTF-8 view of the base and path is the exact parser input.
    fn request_url(&self, model: &str) -> Result<String, GoogleRequestError> {
        let model = transform_model_name(self.endpoint.vertexai, model)?;
        let path = self.stream_request_path(&model);
        let base = encode_node_utf8(&self.endpoint.base_url);
        let mut base = base.strip_suffix('/').unwrap_or(&base).to_owned();
        if !self.endpoint.api_version.is_empty() {
            base.push('/');
            base.push_str(&self.endpoint.api_version);
        }
        let raw_url = format!("{base}/{}", encode_node_utf8(&path));
        let mut url = url::Url::parse(&raw_url).map_err(|_| GoogleRequestError::new("Invalid URL", None))?;
        set_alt_to_sse(&mut url);
        Ok(url.into())
    }

    fn stream_request_path(&self, model: &str) -> JsString {
        let path = JsString::from(format!("{model}:streamGenerateContent?alt=sse"));
        let uses_adc = self.endpoint.vertexai && self.api_key.is_none();
        let skip_project_prefix = self.endpoint.collection_scope || model.starts_with("projects/");
        if uses_adc && !skip_project_prefix {
            let empty = JsString::default();
            JsString::from("projects/")
                + self.endpoint.project.as_ref().unwrap_or(&empty)
                + "/locations/"
                + self.endpoint.location.as_ref().unwrap_or(&empty)
                + "/"
                + path
        } else {
            path
        }
    }

    async fn request_headers(
        &self,
        fetch: &dyn Fetch,
        signal: Option<&AbortSignal>,
    ) -> Result<HeaderList, GoogleRequestError> {
        let mut entries: Vec<(String, JsString)> = vec![
            ("User-Agent".to_owned(), SDK_USER_AGENT.into()),
            ("x-goog-api-client".to_owned(), SDK_USER_AGENT.into()),
            ("Content-Type".to_owned(), "application/json".into()),
        ];
        for (name, value) in &self.headers {
            match entries.iter_mut().find(|(existing, _)| existing == name) {
                Some((_, existing)) => *existing = value.clone(),
                None => entries.push((name.clone(), value.clone())),
            }
        }
        let mut headers = HeaderList::from_entries(entries)?;
        if let Some(api_key) = &self.api_key {
            if !headers.contains("x-goog-api-key") {
                headers.append("x-goog-api-key", api_key)?;
            }
        } else if !headers.contains("authorization") {
            let token = self.fetch_access_token(fetch, signal).await?;
            headers.append("authorization", &JsString::from(format!("Bearer {token}")))?;
        }
        Ok(headers)
    }

    async fn fetch_access_token(
        &self,
        fetch: &dyn Fetch,
        _signal: Option<&AbortSignal>,
    ) -> Result<String, GoogleRequestError> {
        let now = now_millis_f64();
        if let Some((token, expires_at)) = self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            && *expires_at > now
        {
            return Ok(token.clone());
        }
        let credentials = load_credentials(self.key_filename.as_ref())?;
        let (body, accept) = token_request_body(&credentials)?;
        let request = HttpRequest {
            method: "POST".to_owned(),
            url: GOOGLE_TOKEN_URL.to_owned(),
            headers: vec![
                ("accept".to_owned(), accept.to_owned()),
                (
                    "content-type".to_owned(),
                    "application/x-www-form-urlencoded;charset=UTF-8".to_owned(),
                ),
            ],
            body: Some(body.into_bytes()),
        };
        // google-auth-library's transporter has no AbortSignal, so the token
        // exchange is never cancelled by the request signal.
        let response = fetch
            .fetch(request, None)
            .await
            .map_err(|error| GoogleRequestError::new(error.message, None))?;
        let status = response.status;
        let text = read_body_text(response.body).await;
        let parsed: Option<Value> = serde_json::from_str(&text).ok();
        if !(200..300).contains(&status) {
            let message = parsed
                .as_ref()
                .and_then(|value| value.get("error"))
                .and_then(Value::as_str)
                .map(|error| {
                    let description = parsed
                        .as_ref()
                        .and_then(|value| value.get("error_description"))
                        .and_then(Value::as_str)
                        .unwrap_or("undefined");
                    format!("{error}: {description}")
                })
                .unwrap_or_else(|| format!("Request failed with status code {status}"));
            return Err(GoogleRequestError::new(message, Some(f64::from(status))));
        }
        let value = parsed.ok_or_else(|| GoogleRequestError::new("invalid token response", Some(f64::from(status))))?;
        let access_token = value
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| GoogleRequestError::new("invalid token response", Some(f64::from(status))))?
            .to_owned();
        let expires_in = value.get("expires_in").and_then(Value::as_f64).unwrap_or(0.0);
        *self.token.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some((access_token.clone(), now + expires_in * 1000.0));
        Ok(access_token)
    }
}

fn google_request_signal(caller: Option<&AbortSignal>) -> Option<AbortSignal> {
    caller.map(|caller| {
        let controller = pi_js_compat::abort::AbortController::new();
        let target = controller.clone();
        caller.add_listener(move |_| target.abort(None));
        controller.signal()
    })
}

impl GoogleStream {
    /// Next parsed stream chunk; `None` at end of stream.
    pub async fn next_chunk(&mut self) -> Option<Result<GoogleChunk, GoogleRequestError>> {
        loop {
            if let Some(event) = take_sse_event(&mut self.buffer)
                && let Some(chunk) = parse_sse_event(&event)
            {
                return Some(chunk);
            }
            if self.done {
                if !self.buffer.trim().is_empty() {
                    return Some(Err(GoogleRequestError::new("Incomplete JSON segment at the end", None)));
                }
                return None;
            }
            match self.body.next_chunk().await {
                None => self.done = true,
                Some(Err(error)) => return Some(Err(GoogleRequestError::new(error.error_or_json_message(), None))),
                Some(Ok(bytes)) => {
                    let text = self.decoder.decode(&bytes);
                    if let Err(error) = check_chunk_error(&text) {
                        return Some(Err(error));
                    }
                    self.buffer.push_str(&text);
                }
            }
        }
    }
}

/// Incremental `TextDecoder('utf-8', {stream: true})`.
#[derive(Default)]
struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        let mut input = std::mem::take(&mut self.pending);
        input.extend_from_slice(bytes);
        match std::str::from_utf8(&input) {
            Ok(text) => text.to_owned(),
            Err(error) if error.error_len().is_none() => {
                let valid = error.valid_up_to();
                self.pending = input[valid..].to_vec();
                String::from_utf8_lossy(&input[..valid]).into_owned()
            }
            Err(_) => String::from_utf8_lossy(&input).into_owned(),
        }
    }
}

fn take_sse_event(buffer: &mut String) -> Option<String> {
    const DELIMITERS: [&str; 3] = ["\n\n", "\r\r", "\r\n\r\n"];
    let mut best: Option<(usize, usize)> = None;
    for delimiter in DELIMITERS {
        if let Some(index) = buffer.find(delimiter)
            && best.is_none_or(|(best_index, _)| index < best_index)
        {
            best = Some((index, delimiter.len()));
        }
    }
    let (index, length) = best?;
    let event = buffer[..index].to_owned();
    buffer.drain(..index + length);
    Some(event)
}

fn parse_sse_event(event: &str) -> Option<Result<GoogleChunk, GoogleRequestError>> {
    let trimmed = event.trim();
    let payload = trimmed.strip_prefix("data:")?.trim();
    Some(
        pi_js_compat::js_json::parse_lossless(payload)
            .map_err(|error| GoogleRequestError::new(error.to_string(), None))
            .and_then(|value| {
                pi_js_compat::js_json::from_value(&value).map_err(|error| {
                    GoogleRequestError::new(format!("exception parsing stream chunk {payload}. {error}"), None)
                })
            }),
    )
}

fn check_chunk_error(text: &str) -> Result<(), GoogleRequestError> {
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(text) else {
        return Ok(());
    };
    let Some(error) = object.get("error") else {
        return Ok(());
    };
    let code = error.get("code").and_then(Value::as_f64);
    let status = error
        .get("status")
        .map(|status| match status {
            Value::String(status) => status.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "undefined".to_owned());
    if let Some(code) = code
        && (400.0..600.0).contains(&code)
    {
        let message = format!("got status: {status}. {}", js_json(&Value::Object(object.clone())));
        return Err(GoogleRequestError::new(message, Some(code)));
    }
    Ok(())
}

async fn read_http_error(response: HttpResponse) -> GoogleRequestError {
    let status = response.status;
    let is_json = response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .is_some_and(|(_, value)| value.contains("application/json"));
    let text = read_body_text(response.body).await;
    let message = if is_json {
        match serde_json::from_str::<Value>(&text) {
            Ok(value) => js_json(&value),
            Err(_) => text,
        }
    } else {
        js_json(&json!({
            "error": {"message": text, "code": status, "status": response.status_text}
        }))
    };
    GoogleRequestError::new(message, Some(f64::from(status)))
}

async fn read_body_text(mut body: Box<dyn HttpBody>) -> String {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        match chunk {
            Ok(chunk) => bytes.extend_from_slice(&chunk),
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn merge_headers(model: &Model, option_headers: Option<&ProviderHeaders>) -> Vec<(String, JsString)> {
    let mut merged: ProviderHeaders = IndexMap::new();
    for (name, value) in model.headers.iter().flatten() {
        merged.insert(name.clone(), Some(value.clone()));
    }
    for (name, value) in option_headers.into_iter().flatten() {
        merged.insert(name.clone(), value.clone());
    }
    provider_headers_to_record(Some(&merged))
        .map(|record| record.into_iter().collect())
        .unwrap_or_default()
}

/// `getEnv()`: trimmed process environment value, empty values are absent.
fn read_process_env_value(name: &str) -> Option<String> {
    pi_js_compat::process_env::get(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn resolve_custom_base_url(base_url: &JsString) -> Option<JsString> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() || trimmed.contains("{location}") {
        return None;
    }
    Some(trimmed)
}

/// ASCII `vN[beta[N]]` segment grammar: an unpaired unit (or its U+FFFD) can
/// never be part of a matching segment, and the separators are ASCII.
fn base_url_includes_api_version(base_url: &JsString) -> bool {
    let base_url = encode_node_utf8(base_url);
    match url::Url::parse(&base_url) {
        Ok(url) => url.path().split('/').any(is_api_version_segment),
        Err(_) => base_url.split('/').any(is_api_version_segment),
    }
}

fn is_api_version_segment(segment: &str) -> bool {
    let Some(rest) = segment.strip_prefix('v') else {
        return false;
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let rest = &rest[digits..];
    rest.is_empty()
        || rest
            .strip_prefix("beta")
            .is_some_and(|beta| beta.chars().all(|c| c.is_ascii_digit()))
}

fn vertex_default_base_url(location: &JsString) -> JsString {
    if location == "global" {
        "https://aiplatform.googleapis.com/".into()
    } else if location == "us" || location == "eu" {
        JsString::from("https://aiplatform.") + location + ".rep.googleapis.com/"
    } else {
        JsString::from("https://") + location + "-aiplatform.googleapis.com/"
    }
}

fn transform_model_name(vertexai: bool, model: &str) -> Result<String, GoogleRequestError> {
    if model.is_empty() {
        return Err(GoogleRequestError::new("model is required and must be a string", None));
    }
    if model.contains("..") || model.contains('?') || model.contains('&') {
        return Err(GoogleRequestError::new("invalid model parameter", None));
    }
    if vertexai {
        if model.starts_with("publishers/") || model.starts_with("projects/") || model.starts_with("models/") {
            return Ok(model.to_owned());
        }
        if let Some((publisher, name)) = model.split_once('/') {
            return Ok(format!("publishers/{publisher}/models/{name}"));
        }
        return Ok(format!("publishers/google/models/{model}"));
    }
    if model.starts_with("models/") || model.starts_with("tunedModels/") {
        return Ok(model.to_owned());
    }
    Ok(format!("models/{model}"))
}

fn set_alt_to_sse(url: &mut url::Url) {
    if url
        .query_pairs()
        .find(|(name, _)| name == "alt")
        .is_some_and(|(_, value)| value == "sse")
    {
        return;
    }
    let mut replaced = false;
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter_map(|(name, value)| {
            if name != "alt" {
                return Some((name.into_owned(), value.into_owned()));
            }
            if replaced {
                return None;
            }
            replaced = true;
            Some(("alt".into(), "sse".into()))
        })
        .collect();
    if !replaced {
        pairs.push(("alt".into(), "sse".into()));
    }
    url.query_pairs_mut().clear().extend_pairs(pairs);
}

/// Transform `GenerateContentParameters` into the wire body.
fn request_body(vertexai: bool, params: &JsValue) -> JsValue {
    let mut body = JsObject::new();
    if let Some(contents) = params.get("contents").and_then(JsValue::as_array) {
        body.insert(
            "contents".into(),
            JsValue::Array(
                contents
                    .iter()
                    .map(|content| transform_content(vertexai, content))
                    .collect(),
            ),
        );
    }
    let config = params.get("config").cloned().unwrap_or(JsValue::Null);
    if let Some(system_instruction) = config.get("systemInstruction") {
        body.insert(
            "systemInstruction".into(),
            transform_system_instruction(vertexai, system_instruction),
        );
    }
    if let Some(tools) = config.get("tools").and_then(JsValue::as_array) {
        body.insert(
            "tools".into(),
            JsValue::Array(tools.iter().map(|tool| transform_tool(vertexai, tool)).collect()),
        );
    }
    if let Some(tool_config) = config.get("toolConfig") {
        body.insert("toolConfig".into(), transform_tool_config(vertexai, tool_config));
    }
    if config.is_object() {
        let mut generation_config = JsObject::new();
        for key in ["temperature", "maxOutputTokens"] {
            if let Some(value) = config.get(key) {
                generation_config.insert(key.into(), value.clone());
            }
        }
        if let Some(value) = config.get("thinkingConfig") {
            generation_config.insert("thinkingConfig".into(), value.clone());
        }
        body.insert("generationConfig".into(), JsValue::Object(generation_config));
    }
    JsValue::Object(body)
}

fn transform_content(vertexai: bool, content: &JsValue) -> JsValue {
    let mut result = JsObject::new();
    if let Some(parts) = content.get("parts").and_then(JsValue::as_array) {
        result.insert(
            "parts".into(),
            JsValue::Array(parts.iter().map(|part| transform_part(vertexai, part)).collect()),
        );
    }
    if let Some(role) = content.get("role") {
        result.insert("role".into(), role.clone());
    }
    JsValue::Object(result)
}

fn transform_part(vertexai: bool, part: &JsValue) -> JsValue {
    let mut result = JsObject::new();
    if let Some(function_call) = part.get("functionCall") {
        result.insert(
            "functionCall".into(),
            if vertexai {
                function_call.clone()
            } else {
                transform_function_call(function_call)
            },
        );
    }
    if let Some(function_response) = part.get("functionResponse") {
        result.insert("functionResponse".into(), function_response.clone());
    }
    if let Some(inline_data) = part.get("inlineData") {
        result.insert(
            "inlineData".into(),
            if vertexai {
                inline_data.clone()
            } else {
                transform_inline_data(inline_data)
            },
        );
    }
    for key in ["text", "thought", "thoughtSignature"] {
        if let Some(value) = part.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    JsValue::Object(result)
}

fn transform_function_call(function_call: &JsValue) -> JsValue {
    let mut result = JsObject::new();
    for key in ["id", "args", "name"] {
        if let Some(value) = function_call.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    JsValue::Object(result)
}

fn transform_inline_data(inline_data: &JsValue) -> JsValue {
    let mut result = JsObject::new();
    for key in ["data", "mimeType"] {
        if let Some(value) = inline_data.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    JsValue::Object(result)
}

fn transform_system_instruction(vertexai: bool, instruction: &JsValue) -> JsValue {
    match instruction {
        JsValue::String(text) => transform_content(
            vertexai,
            &JsValue::object([
                (
                    "parts",
                    JsValue::Array(vec![JsValue::object([("text", JsValue::String(text.clone()))])]),
                ),
                ("role", JsValue::String("user".into())),
            ]),
        ),
        other => transform_content(vertexai, other),
    }
}

fn transform_tool(vertexai: bool, tool: &JsValue) -> JsValue {
    let mut result = JsObject::new();
    if let Some(declarations) = tool.get("functionDeclarations").and_then(JsValue::as_array) {
        result.insert(
            "functionDeclarations".into(),
            JsValue::Array(
                declarations
                    .iter()
                    .map(|declaration| {
                        if vertexai {
                            transform_function_declaration(declaration)
                        } else {
                            declaration.clone()
                        }
                    })
                    .collect(),
            ),
        );
    }
    JsValue::Object(result)
}

fn transform_function_declaration(declaration: &JsValue) -> JsValue {
    let mut result = JsObject::new();
    for key in [
        "description",
        "name",
        "parameters",
        "parametersJsonSchema",
        "response",
        "responseJsonSchema",
    ] {
        if let Some(value) = declaration.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    JsValue::Object(result)
}

fn transform_tool_config(vertexai: bool, tool_config: &JsValue) -> JsValue {
    if vertexai {
        return tool_config.clone();
    }
    let mut result = JsObject::new();
    if let Some(function_calling_config) = tool_config.get("functionCallingConfig") {
        let mut transformed = JsObject::new();
        if let Some(mode) = function_calling_config.get("mode") {
            transformed.insert("mode".into(), mode.clone());
        }
        result.insert("functionCallingConfig".into(), JsValue::Object(transformed));
    }
    JsValue::Object(result)
}

fn now_millis_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as f64
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs() as i64
}

fn token_request_body(credentials: &Credentials) -> Result<(String, &'static str), GoogleRequestError> {
    match credentials {
        Credentials::ServiceAccount {
            client_email,
            private_key,
        } => {
            let assertion = sign_service_account_jwt(client_email, private_key)?;
            let body = [
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ]
            .iter()
            .map(|(name, value)| format!("{name}={}", form_encode(value)))
            .collect::<Vec<_>>()
            .join("&");
            Ok((body, "application/json"))
        }
        Credentials::AuthorizedUser {
            client_id,
            client_secret,
            refresh_token,
        } => {
            let body = [
                ("refresh_token", refresh_token.as_str()),
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("grant_type", "refresh_token"),
            ]
            .iter()
            .map(|(name, value)| format!("{name}={}", form_encode(value)))
            .collect::<Vec<_>>()
            .join("&");
            Ok((body, "*/*"))
        }
    }
}

fn sign_service_account_jwt(client_email: &str, private_key: &str) -> Result<String, GoogleRequestError> {
    let issued_at = now_seconds();
    let mut claims = Map::new();
    claims.insert("iss".to_owned(), Value::String(client_email.to_owned()));
    claims.insert("scope".to_owned(), Value::String(VERTEX_SCOPE.to_owned()));
    claims.insert("aud".to_owned(), Value::String(GOOGLE_TOKEN_URL.to_owned()));
    claims.insert("exp".to_owned(), json!(issued_at + JWT_LIFETIME_SECONDS));
    claims.insert("iat".to_owned(), json!(issued_at));
    let header = json!({"alg": "RS256"});
    let signing_input = format!(
        "{}.{}",
        base64url(header.to_string().as_bytes()),
        base64url(
            serde_json::to_string(&Value::Object(claims))
                .unwrap_or_default()
                .as_bytes()
        )
    );
    let key = rsa::RsaPrivateKey::from_pkcs8_pem(private_key)
        .or_else(|_| rsa::RsaPrivateKey::from_pkcs1_pem(private_key))
        .map_err(|error| GoogleRequestError::new(error.to_string(), None))?;
    let signature = SigningKey::<Sha256>::new(key).sign(signing_input.as_bytes());
    Ok(format!("{signing_input}.{}", base64url(&signature.to_vec())))
}

/// `JSON.stringify` for JSON-shaped values, including JavaScript number formatting.
pub(crate) fn js_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(number) => match (number.as_i64(), number.as_u64()) {
            (Some(value), _) => value.to_string(),
            (_, Some(value)) => value.to_string(),
            _ => pi_js_compat::number::js_number_to_string(number.as_f64().unwrap_or_default()),
        },
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned()),
        Value::Array(values) => {
            let items: Vec<String> = values.iter().map(js_json).collect();
            format!("[{}]", items.join(","))
        }
        Value::Object(object) => {
            let items: Vec<String> = object
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_owned()),
                        js_json(value)
                    )
                })
                .collect();
            format!("{{{}}}", items.join(","))
        }
    }
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(ALPHABET[(third & 0b111111) as usize] as char);
        }
    }
    encoded
}

fn form_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => encoded.push(byte as char),
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn load_credentials(key_filename: Option<&JsString>) -> Result<Credentials, GoogleRequestError> {
    let path = key_filename
        .cloned()
        .or_else(|| get_provider_env_value("GOOGLE_APPLICATION_CREDENTIALS", None))
        .or_else(well_known_adc_path);
    let Some(path) = path else {
        return Err(GoogleRequestError::new(
            "Could not load the default credentials. Browse to https://cloud.google.com/docs/authentication/getting-started for more information.",
            None,
        ));
    };
    let contents = std::fs::read_to_string(encode_node_utf8(&path))
        .map_err(|error| GoogleRequestError::new(format!("Unable to read the credential file: {error}"), None))?;
    parse_credentials(&contents)
}

fn well_known_adc_path() -> Option<JsString> {
    let home = pi_js_compat::process_env::get("HOME")?;
    let path = std::path::Path::new(&home).join(".config/gcloud/application_default_credentials.json");
    path.is_file().then(|| JsString::from(path.to_string_lossy().as_ref()))
}

fn parse_credentials(contents: &str) -> Result<Credentials, GoogleRequestError> {
    let value: Value = pi_js_compat::js_json::parse(contents)
        .map_err(|error| GoogleRequestError::new(format!("Unable to parse the credential file: {error}"), None))?;
    match value.get("type").and_then(Value::as_str) {
        Some("authorized_user") => Ok(Credentials::AuthorizedUser {
            client_id: required_string(&value, "client_id")?,
            client_secret: required_string(&value, "client_secret")?,
            refresh_token: required_string(&value, "refresh_token")?,
        }),
        Some("service_account") | None => Ok(Credentials::ServiceAccount {
            client_email: required_string(&value, "client_email")?,
            private_key: required_string(&value, "private_key")?,
        }),
        Some(other) => Err(GoogleRequestError::new(
            format!("Unsupported application default credential type: {other}"),
            None,
        )),
    }
}

fn required_string(value: &Value, key: &str) -> Result<String, GoogleRequestError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            GoogleRequestError::new(format!("The incoming JSON object does not contain a {key} field"), None)
        })
}

static GENERATIVE_TOOL_CALL_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static VERTEX_TOOL_CALL_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Text,
    Thinking,
}

pub(crate) async fn pump_stream(
    google_stream: &mut GoogleStream,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    model: &Model,
) -> Result<(), GoogleRequestError> {
    let mut current: Option<(BlockKind, usize)> = None;
    while let Some(chunk) = google_stream.next_chunk().await {
        let chunk = chunk?;
        remember_response_id(output, &chunk);
        if let Some(candidate) = first_candidate(&chunk) {
            if let Some(parts) = candidate.content.as_ref().and_then(|content| content.parts.as_ref()) {
                for part in parts {
                    apply_part(sender, output, part, &mut current);
                }
            }
            apply_finish_reason(output, candidate)?;
        }
        apply_usage(output, model, &chunk);
    }
    close_block(sender, output, current);
    Ok(())
}

fn first_candidate(chunk: &GoogleChunk) -> Option<&GoogleCandidate> {
    chunk.candidates.as_ref().and_then(|candidates| candidates.first())
}

fn remember_response_id(output: &mut AssistantMessage, chunk: &GoogleChunk) {
    if !output.response_id.as_deref().unwrap_or_default().is_empty() {
        return;
    }
    if let Some(response_id) = chunk.response_id.clone() {
        output.response_id = Some(response_id);
    }
}

fn apply_part(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    part: &GooglePart,
    current: &mut Option<(BlockKind, usize)>,
) {
    if let Some(text) = &part.text {
        apply_text_part(sender, output, part, text, current);
    }
    if let Some(function_call) = &part.function_call {
        apply_function_call_part(sender, output, part, function_call, current);
    }
}

fn apply_text_part(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    part: &GooglePart,
    text: &JsString,
    current: &mut Option<(BlockKind, usize)>,
) {
    let thinking = is_thinking_part(part.thought);
    if block_kind_differs(*current, thinking) {
        close_block(sender, output, *current);
        *current = Some(open_block(sender, output, thinking));
    }
    let (kind, index) = current.expect("a block was just opened");
    append_text(output, kind, index, text, part.thought_signature.as_deref());
    let event = match kind {
        BlockKind::Text => AssistantMessageEvent::TextDelta {
            content_index: index,
            delta: text.into(),
            partial: output.clone(),
        },
        BlockKind::Thinking => AssistantMessageEvent::ThinkingDelta {
            content_index: index,
            delta: text.to_owned(),
            partial: output.clone(),
        },
    };
    sender.push(event);
}

fn block_kind_differs(current: Option<(BlockKind, usize)>, thinking: bool) -> bool {
    match current {
        None => true,
        Some((BlockKind::Thinking, _)) => !thinking,
        Some((BlockKind::Text, _)) => thinking,
    }
}

fn apply_function_call_part(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    part: &GooglePart,
    function_call: &GoogleFunctionCall,
    current: &mut Option<(BlockKind, usize)>,
) {
    if current.is_some() {
        close_block(sender, output, *current);
        *current = None;
    }
    let mut tool_call = ToolCall::new(
        resolve_tool_call_id(output, function_call),
        function_call.name.clone().unwrap_or_default(),
        tool_call_arguments(function_call),
    );
    tool_call.thought_signature = part.thought_signature.clone().filter(|signature| !signature.is_empty());
    emit_tool_call(sender, output, tool_call);
}

fn resolve_tool_call_id(output: &AssistantMessage, function_call: &GoogleFunctionCall) -> String {
    let provided_id = function_call.id.clone().filter(|id| !id.is_empty());
    let duplicate = provided_id.as_ref().is_some_and(|id| {
        output
            .content
            .iter()
            .any(|block| matches!(block, AssistantContent::ToolCall(call) if &call.id == id))
    });
    match provided_id {
        Some(id) if !duplicate => id,
        _ => generate_tool_call_id(&output.api, function_call.name.as_deref().unwrap_or("undefined")),
    }
}

fn tool_call_arguments(function_call: &GoogleFunctionCall) -> JsObject {
    match &function_call.args {
        Some(JsValue::Object(args)) => args.clone(),
        _ => JsObject::new(),
    }
}

fn emit_tool_call(sender: &AssistantMessageEventSender, output: &mut AssistantMessage, tool_call: ToolCall) {
    let index = output.content.len();
    output.content.push(AssistantContent::ToolCall(tool_call.clone()));
    sender.push(AssistantMessageEvent::ToolCallStart {
        content_index: index,
        partial: output.clone(),
    });
    sender.push(AssistantMessageEvent::ToolCallDelta {
        content_index: index,
        delta: pi_js_compat::js_json::stringify_lossless(&JsValue::Object(tool_call.arguments.clone())).into(),
        partial: output.clone(),
    });
    sender.push(AssistantMessageEvent::ToolCallEnd {
        content_index: index,
        tool_call: Box::new(tool_call),
        partial: output.clone(),
    });
}

fn apply_finish_reason(output: &mut AssistantMessage, candidate: &GoogleCandidate) -> Result<(), GoogleRequestError> {
    let Some(finish_reason) = &candidate.finish_reason else {
        return Ok(());
    };
    output.stop_reason = map_stop_reason(finish_reason).map_err(|message| GoogleRequestError::new(message, None))?;
    if output
        .content
        .iter()
        .any(|block| matches!(block, AssistantContent::ToolCall(_)))
    {
        output.stop_reason = StopReason::ToolUse;
    }
    Ok(())
}

fn apply_usage(output: &mut AssistantMessage, model: &Model, chunk: &GoogleChunk) {
    let Some(usage) = &chunk.usage_metadata else {
        return;
    };
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

fn append_text(output: &mut AssistantMessage, kind: BlockKind, index: usize, text: &JsString, signature: Option<&str>) {
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

fn generate_tool_call_id(api: &str, name: &str) -> String {
    let counter = if api == "google-vertex" {
        &VERTEX_TOOL_CALL_COUNTER
    } else {
        &GENERATIVE_TOOL_CALL_COUNTER
    };
    let counter = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    format!("{name}_{}_{counter}", now_millis_f64() as i64)
}

/// Resolve the `done` reason shared by both adapters.
pub(crate) fn completed_reason(output: &AssistantMessage, signal: Option<&AbortSignal>) -> Result<DoneReason, String> {
    if signal.is_some_and(AbortSignal::aborted) {
        return Err("Request was aborted".to_owned());
    }
    if matches!(output.stop_reason, StopReason::Aborted | StopReason::Error) {
        return Err("An unknown error occurred".to_owned());
    }
    match output.stop_reason {
        StopReason::Stop => Ok(DoneReason::Stop),
        StopReason::Length => Ok(DoneReason::Length),
        StopReason::ToolUse => Ok(DoneReason::ToolUse),
        StopReason::Error | StopReason::Aborted => Err("An unknown error occurred".to_owned()),
    }
}

/// Open the SDK stream without a provider retry wrapper, matching frozen callers.
pub(crate) async fn open_generate_stream(
    client: &GoogleClient,
    params: &JsValue,
    request: &crate::request::ProviderRequestOptions,
) -> Result<GoogleStream, GoogleRequestError> {
    let fetch = request.fetch.clone().unwrap_or_else(default_fetch);
    client
        .generate_content_stream(params, fetch.as_ref(), request.signal.as_ref())
        .await
}
