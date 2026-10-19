//! Port of `packages/coding-agent/src/extensions/llama/client.ts`.
//!
//! The llama.cpp router speaks plain HTTP; every request goes through the
//! injected [`Fetch`] contract so tests never need a live server.

use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use pi_ai::request::{CallbackError, Fetch, HttpBody, HttpRequest};
use pi_js_compat::number::{js_number_to_string, to_fixed};
use pi_js_compat::string::js_trim_start;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::utils::management_http::{RequestDeadline, throw_if_aborted};

/// `AbortSignal.timeout(15_000)` on every management request.
const REQUEST_TIMEOUT_MS: u64 = 15_000;

pub type LlamaResult<T> = Result<T, CallbackError>;

/// `LlamaModelStatus.value`. The source type is a closed union, but the wire
/// value is only validated as a string by `isModelInfo`; keeping it as a string
/// preserves acceptance of router statuses the union does not enumerate.
pub const STATUS_UNLOADED: &str = "unloaded";
pub const STATUS_LOADING: &str = "loading";
pub const STATUS_LOADED: &str = "loaded";
pub const STATUS_DOWNLOADING: &str = "downloading";
pub const STATUS_SLEEPING: &str = "sleeping";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaModelStatus {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<IndexMap<String, LlamaDownloadProgress>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaDownloadProgress {
    pub done: f64,
    pub total: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaArchitecture {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_modalities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaModelMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_ctx: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_ctx_train: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ftype: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaModelInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    pub status: LlamaModelStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<LlamaArchitecture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<LlamaModelMeta>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaModelsResponse {
    pub data: Vec<LlamaModelInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlamaModelEvent {
    pub model: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlamaProgress {
    pub message: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// `list(options = {})`.
#[derive(Debug, Clone, Default)]
pub struct LlamaListOptions {
    pub reload: bool,
    pub signal: Option<CancellationToken>,
}

fn error_message(payload: Option<&Value>, fallback: &str) -> String {
    let message = payload
        .and_then(|payload| payload.get("error"))
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str);
    match message {
        Some(message) if !message.is_empty() => message.to_owned(),
        _ => fallback.to_owned(),
    }
}

fn is_model_info(value: &Value) -> bool {
    value.get("id").is_some_and(Value::is_string)
        && value
            .get("status")
            .and_then(|status| status.get("value"))
            .is_some_and(Value::is_string)
}

pub fn format_bytes(bytes: f64) -> String {
    if bytes < 1024.0 {
        return format!("{} B", js_number_to_string(bytes));
    }
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes / 1024.0;
    let mut unit = UNITS[0];
    let mut index = 1;
    while index < UNITS.len() && value >= 1024.0 {
        value /= 1024.0;
        unit = UNITS[index];
        index += 1;
    }
    let digits = if value >= 10.0 { 1 } else { 2 };
    format!("{} {unit}", to_fixed(value, digits))
}

pub fn normalize_llama_server_url(value: &str) -> LlamaResult<String> {
    let trimmed = pi_js_compat::string::js_trim(value);
    let mut url = url::Url::parse(trimmed).map_err(|_| CallbackError::new("Invalid URL"))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(CallbackError::new("Server URL must use http or https"));
    }
    url.set_fragment(None);
    url.set_query(None);
    let path = url.path().trim_end_matches('/').to_owned();
    let path = path.strip_suffix("/v1").unwrap_or(&path);
    url.set_path(if path.is_empty() { "/" } else { path });
    let serialized = url.to_string();
    Ok(serialized.strip_suffix('/').unwrap_or(&serialized).to_owned())
}

pub fn llama_inference_url(server_url: &str) -> LlamaResult<String> {
    Ok(format!("{}/v1", normalize_llama_server_url(server_url)?))
}

fn parse_load_progress(data: Option<&Value>) -> Option<LlamaProgress> {
    let progress = data?.get("progress")?;
    if !(progress.is_object() || progress.is_array()) {
        return None;
    }
    let stage = progress
        .get("current")
        .and_then(Value::as_str)
        .or_else(|| progress.get("stage").and_then(Value::as_str));
    let stages: Vec<&str> = progress
        .get("stages")
        .and_then(Value::as_array)
        .map(|stages| stages.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let stage_ratio = progress
        .get("value")
        .and_then(Value::as_f64)
        .map(|value| value.clamp(0.0, 1.0));
    let mut ratio = stage_ratio;
    if let Some(stage) = stage
        && !stages.is_empty()
        && let Some(index) = stages.iter().position(|entry| *entry == stage)
    {
        ratio = Some((index as f64 + stage_ratio.unwrap_or(0.0)) / stages.len() as f64);
    }
    Some(LlamaProgress {
        message: stage.map_or_else(
            || "Loading model".to_owned(),
            |stage| format!("Loading {}", stage.replace('_', " ")),
        ),
        ratio,
        detail: None,
    })
}

fn finish_download_progress(done: f64, total: f64) -> Option<LlamaProgress> {
    if total <= 0.0 {
        return None;
    }
    Some(LlamaProgress {
        message: "Downloading model".to_owned(),
        ratio: Some(done / total),
        detail: Some(format!("{} / {}", format_bytes(done), format_bytes(total))),
    })
}

fn parse_download_progress(data: &Value) -> Option<LlamaProgress> {
    let nested = data.get("progress");
    let files = match nested {
        Some(nested) if nested.is_object() || nested.is_array() => nested,
        _ => data,
    };
    let entries: Box<dyn Iterator<Item = &Value> + '_> = if let Some(files) = files.as_object() {
        Box::new(files.values())
    } else {
        Box::new(files.as_array()?.iter())
    };
    let mut done = 0.0;
    let mut total = 0.0;
    for entry in entries {
        let Some(done_value) = entry.get("done").and_then(Value::as_f64) else {
            continue;
        };
        let Some(total_value) = entry.get("total").and_then(Value::as_f64) else {
            continue;
        };
        done += done_value;
        total += total_value;
    }
    finish_download_progress(done, total)
}

fn download_progress_from_counts(progress: &IndexMap<String, LlamaDownloadProgress>) -> Option<LlamaProgress> {
    let mut done = 0.0;
    let mut total = 0.0;
    for entry in progress.values() {
        done += entry.done;
        total += entry.total;
    }
    finish_download_progress(done, total)
}

/// `AbortSignal.reason ?? new Error("Cancelled")`. `CancellationToken` carries
/// no reason, so the callers' `Error("Cancelled")` message is used.
fn cancelled() -> CallbackError {
    CallbackError::new("Cancelled")
}

async fn sleep(ms: u64, signal: Option<&CancellationToken>) -> LlamaResult<()> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Err(cancelled());
    }
    tokio::select! {
        biased;
        _ = async {
            match signal {
                Some(signal) => signal.cancelled().await,
                None => std::future::pending().await,
            }
        } => Err(cancelled()),
        _ = tokio::time::sleep(std::time::Duration::from_millis(ms)) => Ok(()),
    }
}

async fn read_body(
    mut body: Box<dyn HttpBody>,
    signal: Option<&CancellationToken>,
    deadline: Option<&RequestDeadline>,
) -> LlamaResult<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        throw_if_aborted(signal, deadline)?;
        match body.next_chunk().await {
            Some(Ok(chunk)) => bytes.extend_from_slice(&chunk),
            Some(Err(error)) => {
                throw_if_aborted(signal, deadline)?;
                return Err(error);
            }
            None => return Ok(bytes),
        }
    }
}

/// Sends a management request and decodes the JSON payload.
///
/// A JSON decode failure yields `None`, matching the source's empty `catch`.
pub(crate) async fn request_json(
    fetch: &dyn Fetch,
    request: HttpRequest,
    signal: Option<&CancellationToken>,
) -> LlamaResult<(u16, Vec<(String, String)>, Option<Value>)> {
    let deadline = RequestDeadline::start(signal, Some(REQUEST_TIMEOUT_MS));
    let fetch_signal = deadline.as_ref().map(|deadline| deadline.signal().clone());
    let response = fetch.fetch(request, fetch_signal).await;
    throw_if_aborted(signal, deadline.as_ref())?;
    let response = response?;
    let status = response.status;
    let headers = response.headers;
    let bytes = read_body(response.body, signal, deadline.as_ref()).await?;
    throw_if_aborted(signal, deadline.as_ref())?;
    Ok((status, headers, serde_json::from_slice(&bytes).ok()))
}

/// Incremental `TextDecoder` (non-fatal UTF-8) for the SSE stream.
#[derive(Default)]
struct StreamingTextDecoder {
    pending: Vec<u8>,
}

impl StreamingTextDecoder {
    fn decode(&mut self, chunk: &[u8]) -> String {
        self.pending.extend_from_slice(chunk);
        let mut decoded = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    decoded.push_str(text);
                    self.pending.clear();
                    return decoded;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    decoded.push_str(
                        std::str::from_utf8(&self.pending[..valid]).expect("from_utf8 reported this prefix as valid"),
                    );
                    match error.error_len() {
                        None => {
                            self.pending.drain(..valid);
                            return decoded;
                        }
                        Some(length) => {
                            decoded.push('\u{FFFD}');
                            self.pending.drain(..valid + length);
                        }
                    }
                }
            }
        }
    }
}

/// Aborts the SSE watcher when `loadAndWait`/`downloadAndWait` leave their loop.
struct WatcherGuard(CancellationToken);

impl Drop for WatcherGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub struct LlamaClient {
    server_url: String,
    api_key: Option<String>,
    fetch: Arc<dyn Fetch>,
}

impl Clone for LlamaClient {
    fn clone(&self) -> Self {
        Self {
            server_url: self.server_url.clone(),
            api_key: self.api_key.clone(),
            fetch: Arc::clone(&self.fetch),
        }
    }
}

impl LlamaClient {
    pub fn new(server_url: &str, api_key: Option<&str>) -> LlamaResult<Self> {
        Self::with_fetch(server_url, api_key, pi_ai::http::default_fetch())
    }

    pub fn with_fetch(server_url: &str, api_key: Option<&str>, fetch: Arc<dyn Fetch>) -> LlamaResult<Self> {
        Ok(Self {
            server_url: normalize_llama_server_url(server_url)?,
            api_key: api_key.map(str::to_owned),
            fetch,
        })
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    fn headers(&self, body: bool) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if body {
            headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
        }
        if let Some(api_key) = &self.api_key {
            headers.push(("Authorization".to_owned(), format!("Bearer {api_key}")));
        }
        headers
    }

    async fn request(
        &self,
        path: &str,
        method: &str,
        body: Option<String>,
        signal: Option<&CancellationToken>,
    ) -> LlamaResult<Value> {
        let request = HttpRequest {
            method: method.to_owned(),
            url: format!("{}{path}", self.server_url),
            headers: self.headers(body.is_some()),
            body: body.map(String::into_bytes),
        };
        let (status, _headers, payload) = request_json(self.fetch.as_ref(), request, signal).await?;
        if !(200..300).contains(&status) {
            return Err(CallbackError::new(error_message(
                payload.as_ref(),
                &format!("llama.cpp returned HTTP {status}"),
            )));
        }
        Ok(payload.unwrap_or(Value::Null))
    }

    pub async fn list(&self, options: LlamaListOptions) -> LlamaResult<Vec<LlamaModelInfo>> {
        let reload = if options.reload { "?reload=1" } else { "" };
        let payload = self
            .request(&format!("/models{reload}"), "GET", None, options.signal.as_ref())
            .await?;
        let Some(data) = payload.get("data").and_then(Value::as_array) else {
            return Err(CallbackError::new("llama.cpp returned an invalid model catalog"));
        };
        if !data.iter().all(is_model_info) {
            return Err(CallbackError::new("Server is not running in llama.cpp router mode"));
        }
        data.iter()
            .map(|entry| serde_json::from_value(entry.clone()).map_err(|error| CallbackError::new(error.to_string())))
            .collect()
    }

    pub async fn load(&self, model: &str, signal: Option<&CancellationToken>) -> LlamaResult<()> {
        let body =
            serde_json::to_string(&serde_json::json!({ "model": model })).expect("a model name always serializes");
        self.request("/models/load", "POST", Some(body), signal).await?;
        Ok(())
    }

    pub async fn unload(&self, model: &str, signal: Option<&CancellationToken>) -> LlamaResult<()> {
        let body =
            serde_json::to_string(&serde_json::json!({ "model": model })).expect("a model name always serializes");
        self.request("/models/unload", "POST", Some(body), signal).await?;
        Ok(())
    }

    pub async fn unload_and_wait(&self, model: &str, signal: Option<&CancellationToken>) -> LlamaResult<()> {
        self.unload(model, signal).await?;
        loop {
            let entry = self
                .list(LlamaListOptions {
                    reload: false,
                    signal: signal.cloned(),
                })
                .await?
                .into_iter()
                .find(|candidate| candidate.id == model);
            match entry {
                None => return Ok(()),
                Some(entry) if entry.status.value == STATUS_UNLOADED => return Ok(()),
                Some(_) => sleep(100, signal).await?,
            }
        }
    }

    pub async fn download(&self, model: &str, signal: Option<&CancellationToken>) -> LlamaResult<()> {
        let body =
            serde_json::to_string(&serde_json::json!({ "model": model })).expect("a model name always serializes");
        self.request("/models", "POST", Some(body), signal).await?;
        Ok(())
    }

    pub async fn watch(
        &self,
        on_event: Arc<dyn Fn(LlamaModelEvent) + Send + Sync>,
        signal: Option<CancellationToken>,
    ) -> LlamaResult<()> {
        let request = HttpRequest {
            method: "GET".to_owned(),
            url: format!("{}/models/sse", self.server_url),
            headers: self.headers(false),
            body: None,
        };
        let response = self.fetch.fetch(request, signal).await?;
        if !(200..300).contains(&response.status) {
            return Err(CallbackError::new(format!(
                "llama.cpp SSE returned HTTP {}",
                response.status
            )));
        }
        let mut body = response.body;
        let mut decoder = StreamingTextDecoder::default();
        let mut buffer = String::new();
        loop {
            let Some(chunk) = body.next_chunk().await else {
                return Ok(());
            };
            let chunk = chunk?;
            buffer.push_str(&decoder.decode(&chunk).replace("\r\n", "\n"));
            while let Some(boundary) = buffer.find("\n\n") {
                let frame = buffer[..boundary].to_owned();
                buffer.drain(..boundary + 2);
                let data = frame
                    .split('\n')
                    .filter(|line| line.starts_with("data:"))
                    .map(|line| js_trim_start(&line[5..]))
                    .collect::<Vec<_>>()
                    .join("\n");
                if data.is_empty() {
                    continue;
                }
                let Ok(event) = serde_json::from_str::<LlamaModelEvent>(&data) else {
                    continue;
                };
                on_event(event);
            }
        }
    }

    pub async fn load_and_wait(
        &self,
        model: &str,
        on_progress: Arc<dyn Fn(LlamaProgress) + Send + Sync>,
        signal: Option<CancellationToken>,
    ) -> LlamaResult<LlamaModelInfo> {
        let watcher = signal
            .as_ref()
            .map_or_else(CancellationToken::new, CancellationToken::child_token);
        let _guard = WatcherGuard(watcher.clone());
        let state = Arc::new(Mutex::new(LoadWatchState::default()));
        let watch_state = Arc::clone(&state);
        let watch_client = self.clone();
        let watch_model = model.to_owned();
        let watch_progress = Arc::clone(&on_progress);
        tokio::spawn(async move {
            let result = watch_client
                .watch(
                    Arc::new(move |event| handle_load_event(&watch_model, &event, &watch_state, &watch_progress)),
                    Some(watcher),
                )
                .await;
            // The source discards watcher failures; catalog polling stays authoritative.
            let _ = result;
        });

        self.load(model, signal.as_ref()).await?;
        on_progress(LlamaProgress {
            message: "Loading model".to_owned(),
            ratio: None,
            detail: None,
        });
        loop {
            if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(cancelled());
            }
            let entry = self
                .list(LlamaListOptions {
                    reload: false,
                    signal: signal.clone(),
                })
                .await?
                .into_iter()
                .find(|candidate| candidate.id == model);
            if entry.as_ref().is_some_and(|entry| entry.status.value == STATUS_LOADED) {
                return Ok(entry.expect("checked above"));
            }
            let (event_loaded, event_error) = {
                let state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                (state.loaded, state.error.clone())
            };
            if event_loaded && entry.is_none() {
                return Ok(LlamaModelInfo {
                    id: model.to_owned(),
                    status: LlamaModelStatus {
                        value: STATUS_LOADED.to_owned(),
                        ..LlamaModelStatus::default()
                    },
                    ..LlamaModelInfo::default()
                });
            }
            let failed = entry.as_ref().is_some_and(|entry| entry.status.failed == Some(true));
            if failed || event_error.is_some() {
                let exit_code = entry.as_ref().and_then(|entry| entry.status.exit_code);
                return Err(CallbackError::new(match exit_code {
                    Some(exit_code) => format!("Model exited with code {}", js_number_to_string(exit_code)),
                    None => event_error.unwrap_or_else(|| "Model failed to load".to_owned()),
                }));
            }
            sleep(250, signal.as_ref()).await?;
        }
    }

    pub async fn download_and_wait(
        &self,
        model: &str,
        on_progress: Arc<dyn Fn(LlamaProgress) + Send + Sync>,
        signal: Option<CancellationToken>,
    ) -> LlamaResult<Vec<LlamaModelInfo>> {
        let watcher = signal
            .as_ref()
            .map_or_else(CancellationToken::new, CancellationToken::child_token);
        let _guard = WatcherGuard(watcher.clone());
        let state = Arc::new(Mutex::new(DownloadWatchState::default()));
        let watch_state = Arc::clone(&state);
        let watch_client = self.clone();
        let watch_model = model.to_owned();
        let watch_progress = Arc::clone(&on_progress);
        tokio::spawn(async move {
            let result = watch_client
                .watch(
                    Arc::new(move |event| handle_download_event(&watch_model, &event, &watch_state, &watch_progress)),
                    Some(watcher),
                )
                .await;
            // The source discards watcher failures; catalog polling stays authoritative.
            let _ = result;
        });

        self.download(model, signal.as_ref()).await?;
        on_progress(LlamaProgress {
            message: "Downloading model".to_owned(),
            ratio: None,
            detail: None,
        });
        let mut polls = 0_u32;
        loop {
            if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(cancelled());
            }
            let (finished, failure, saw_downloading) = {
                let state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                (state.finished, state.failure.clone(), state.saw_downloading)
            };
            if let Some(failure) = failure {
                return Err(CallbackError::new(failure));
            }
            let models = self
                .list(LlamaListOptions {
                    reload: false,
                    signal: signal.clone(),
                })
                .await?;
            polls += 1;
            let entry = models.into_iter().find(|candidate| candidate.id == model);
            if entry
                .as_ref()
                .is_some_and(|entry| entry.status.value == STATUS_DOWNLOADING)
            {
                {
                    let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    state.saw_downloading = true;
                }
                if let Some(progress) = entry
                    .as_ref()
                    .and_then(|entry| entry.status.progress.as_ref())
                    .and_then(download_progress_from_counts)
                {
                    on_progress(progress);
                }
            } else if finished || (entry.is_some() && (saw_downloading || polls >= 2)) {
                return self
                    .list(LlamaListOptions {
                        reload: true,
                        signal: signal.clone(),
                    })
                    .await;
            }
            sleep(500, signal.as_ref()).await?;
        }
    }
}

#[derive(Default)]
struct LoadWatchState {
    loaded: bool,
    error: Option<String>,
}

fn handle_load_event(
    model: &str,
    event: &LlamaModelEvent,
    state: &Mutex<LoadWatchState>,
    on_progress: &Arc<dyn Fn(LlamaProgress) + Send + Sync>,
) {
    if event.model != model {
        return;
    }
    if event.event != "model_status" && event.event != "status_change" {
        return;
    }
    let status = event.data.as_ref().and_then(|data| data.get("status"));
    {
        let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match status.and_then(Value::as_str) {
            Some(STATUS_LOADED) => state.loaded = true,
            Some(STATUS_UNLOADED) => state.error = Some("Model failed to load".to_owned()),
            _ => {}
        }
    }
    if let Some(progress) = parse_load_progress(event.data.as_ref()) {
        on_progress(progress);
    }
}

#[derive(Default)]
struct DownloadWatchState {
    finished: bool,
    failure: Option<String>,
    saw_downloading: bool,
}

fn handle_download_event(
    model: &str,
    event: &LlamaModelEvent,
    state: &Mutex<DownloadWatchState>,
    on_progress: &Arc<dyn Fn(LlamaProgress) + Send + Sync>,
) {
    if event.model != model {
        return;
    }
    match event.event.as_str() {
        "download_finished" => {
            state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).finished = true;
        }
        "download_failed" => {
            let failure = error_message(event.data.as_ref(), "Download failed");
            state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).failure = Some(failure);
        }
        "download_progress" => {
            state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .saw_downloading = true;
            if let Some(progress) = event.data.as_ref().and_then(parse_download_progress) {
                on_progress(progress);
            }
        }
        _ => {}
    }
}

/// Boxed variant used by the extension command's progress callback.
pub type LlamaProgressCallback = Arc<dyn Fn(LlamaProgress) + Send + Sync>;
/// Boxed event variant used by the SSE watcher.
pub type LlamaEventCallback = Arc<dyn Fn(LlamaModelEvent) + Send + Sync>;
