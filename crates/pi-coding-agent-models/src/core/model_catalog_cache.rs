//! Frozen `core/model-catalog-cache.ts`: OpenRouter's startup cache and explicit refresh.
//! Raw catalog objects retain unknown properties and UTF-16 until a typed consumer boundary.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use pi_ai::request::{CallbackError, Fetch, HttpRequest, HttpResponse, read_body_text};
use pi_js_compat::abort::{AbortReason, AbortSignal};
use pi_js_compat::date::{date_parse_utf16, js_now_millis};
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use serde::Serialize;

const BASE_URL: &str = "https://openrouter.ai/api/v1";
const MAX_AGE_MS: f64 = 604_800_000.0;
const FETCH_TIMEOUT_MS: u64 = 5000;

pub struct EnsureModelCatalogOptions {
    pub force: bool,
    pub fetch: Arc<dyn Fetch>,
    pub now: Arc<dyn Fn() -> f64 + Send + Sync>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogRefreshResult {
    pub models: Vec<JsValue>,
    pub source: &'static str,
    pub fetched_count: usize,
    pub cached_count: usize,
    pub bundled_count: usize,
    pub cache_path: PathBuf,
}

struct CachedCatalog {
    fetched_at: JsString,
    models: Vec<JsValue>,
}

/// One module instance; production shares this instance between startup and registry reads.
pub struct ModelCatalogCache {
    bundled: Vec<JsValue>,
    memoized: Mutex<Option<Vec<JsValue>>>,
}

impl Default for ModelCatalogCache {
    fn default() -> Self {
        let bundled = pi_ai::compat::get_models("openrouter")
            .into_iter()
            .map(|model| {
                js_json::parse_lossless(serde_json::to_string(&model).expect("catalog model serializes"))
                    .expect("serialized model is JSON")
            })
            .collect();
        Self {
            bundled,
            memoized: Mutex::new(None),
        }
    }
}

impl ModelCatalogCache {
    pub fn get_models(&self) -> Vec<JsValue> {
        self.memoized
            .lock()
            .expect("catalog lock poisoned")
            .clone()
            .unwrap_or_else(|| self.bundled.clone())
    }

    pub async fn load_at_startup(&self, path: &Path, now: impl Fn() -> f64) -> ModelCatalogRefreshResult {
        let cached = read_cached_catalog(path).await;
        let count = cached.as_ref().map_or(0, |catalog| catalog.models.len());
        if is_fresh(cached.as_ref(), now()) {
            let cached = cached.expect("fresh cache exists");
            self.build_result(cached.models, path, "cache", 0, count)
        } else {
            self.build_result(Vec::new(), path, "bundled", 0, count)
        }
    }

    pub async fn ensure_fresh(
        &self,
        path: &Path,
        options: EnsureModelCatalogOptions,
    ) -> Result<ModelCatalogRefreshResult, NativeNodeError> {
        let cached = read_cached_catalog(path).await;
        if !options.force && is_fresh(cached.as_ref(), (options.now)()) {
            let cached = cached.expect("fresh cache exists");
            let count = cached.models.len();
            return Ok(self.build_result(cached.models, path, "cache", 0, count));
        }
        let models = fetch_catalog_with_retries(options.fetch.as_ref()).await?;
        let count = models.len();
        write_cached_catalog(path, (options.now)(), &models).await?;
        Ok(self.build_result(
            models,
            path,
            "network",
            count,
            cached.as_ref().map_or(0, |cached| cached.models.len()),
        ))
    }

    fn build_result(
        &self,
        refreshed: Vec<JsValue>,
        path: &Path,
        source: &'static str,
        fetched_count: usize,
        cached_count: usize,
    ) -> ModelCatalogRefreshResult {
        let mut by_id: indexmap::IndexMap<JsString, JsValue> = self
            .bundled
            .iter()
            .map(|model| {
                let id = model.get("id").and_then(JsValue::as_string).expect("bundled model id");
                (id.clone(), model.clone())
            })
            .collect();
        for model in refreshed {
            let id = model
                .get("id")
                .and_then(JsValue::as_string)
                .expect("validated model id")
                .clone();
            by_id.entry(id).or_insert(model);
        }
        let models: Vec<_> = by_id.into_values().collect();
        *self.memoized.lock().expect("catalog lock poisoned") = Some(models.clone());
        ModelCatalogRefreshResult {
            models,
            source,
            fetched_count,
            cached_count,
            bundled_count: self.bundled.len(),
            cache_path: path.to_owned(),
        }
    }
}

static MODULE_CATALOG: OnceLock<ModelCatalogCache> = OnceLock::new();

fn module_catalog() -> &'static ModelCatalogCache {
    MODULE_CATALOG.get_or_init(ModelCatalogCache::default)
}

fn openrouter_cache_path() -> Result<PathBuf, NativeNodeError> {
    let root = crate::config::get_user_cache_root().map_err(crate::config::ConfigError::into_native_error)?;
    let models = crate::utils::paths::node_join(root, "models");
    let path = crate::utils::paths::node_join(models, "openrouter.json");
    Ok(PathBuf::from(js_json::encode_node_utf8(&path)))
}

pub fn get_openrouter_catalog_models() -> Vec<JsValue> {
    module_catalog().get_models()
}

pub async fn load_openrouter_catalog_at_startup() -> Result<ModelCatalogRefreshResult, NativeNodeError> {
    let path = openrouter_cache_path()?;
    Ok(module_catalog().load_at_startup(&path, js_now_millis).await)
}

pub async fn ensure_model_catalog_fresh(
    options: EnsureModelCatalogOptions,
) -> Result<ModelCatalogRefreshResult, NativeNodeError> {
    let path = openrouter_cache_path()?;
    module_catalog().ensure_fresh(&path, options).await
}

fn is_fresh(cached: Option<&CachedCatalog>, now: f64) -> bool {
    cached.is_some_and(|cached| {
        pi_js_compat::date::time_clip(now) - date_parse_utf16(cached.fetched_at.units()) < MAX_AGE_MS
    })
}

async fn read_cached_catalog(path: &Path) -> Option<CachedCatalog> {
    let content = tokio::fs::read(path).await.ok()?;
    // Node readFile(..., "utf8") decodes malformed bytes before JSON.parse.
    let value = js_json::parse_lossless(String::from_utf8_lossy(&content).as_ref()).ok()?;
    let fetched_at = value.get("fetchedAt")?.as_string()?.clone();
    let models = value.get("models")?.as_array()?;
    if !date_parse_utf16(fetched_at.units()).is_finite() || !models.iter().all(is_cached_model) {
        return None;
    }
    Some(CachedCatalog {
        fetched_at,
        models: models.clone(),
    })
}

fn finite(value: Option<&JsValue>) -> Option<f64> {
    value.and_then(JsValue::as_f64).filter(|number| number.is_finite())
}

fn is_cached_model(value: &JsValue) -> bool {
    let valid_cost = value.get("cost").is_some_and(|cost| {
        ["input", "output", "cacheRead", "cacheWrite"]
            .iter()
            .all(|key| finite(cost.get(key)).is_some())
    });
    let valid_input = value.get("input").and_then(JsValue::as_array).is_some_and(|input| {
        input.iter().all(|value| {
            value
                .as_string()
                .is_some_and(|value| value == "text" || value == "image")
        })
    });
    let valid_limits = ["contextWindow", "maxTokens"]
        .iter()
        .all(|key| finite(value.get(key)).is_some_and(|number| number > 0.0));
    has_valid_cached_identity(value) && valid_cost && valid_input && valid_limits
}

fn has_valid_cached_identity(value: &JsValue) -> bool {
    let valid_names = ["id", "name"].iter().all(|key| {
        value
            .get(key)
            .and_then(JsValue::as_string)
            .is_some_and(|text| !text.units().is_empty())
    });
    let valid_tags = [
        ("api", "openai-completions"),
        ("provider", "openrouter"),
        ("baseUrl", BASE_URL),
    ]
    .iter()
    .all(|(key, text)| {
        value
            .get(key)
            .and_then(JsValue::as_string)
            .is_some_and(|value| value == *text)
    });
    valid_names && valid_tags && value.get("reasoning").and_then(JsValue::as_bool).is_some()
}

fn parse_cost(value: Option<&JsValue>) -> f64 {
    let value = match value {
        Some(JsValue::Number(value)) => *value,
        Some(JsValue::String(value)) => pi_js_compat::number::parse_float(&js_json::encode_node_utf8(value)),
        _ => 0.0,
    };
    if !value.is_finite() {
        return 0.0;
    }
    pi_js_compat::number::to_fixed(value * 1_000_000.0, 6)
        .parse()
        .expect("toFixed returns a number")
}

fn positive_or(value: Option<&JsValue>) -> f64 {
    finite(value).filter(|number| *number > 0.0).unwrap_or(4096.0)
}

fn map_model(value: &JsValue) -> Option<JsValue> {
    let id = value.get("id")?.as_string()?;
    let parameters = value.get("supported_parameters").and_then(JsValue::as_array)?;
    let supports = |parameter: &str| {
        parameters
            .iter()
            .any(|value| value.as_string().is_some_and(|value| value == parameter))
    };
    if !supports("tools") {
        return None;
    }
    let image = value
        .get("architecture")
        .and_then(|value| value.get("modality"))
        .and_then(JsValue::as_string)
        .is_some_and(|value| value.units().windows(5).any(|units| units == [105, 109, 97, 103, 101]));
    let input = if image { vec!["text", "image"] } else { vec!["text"] };
    let input = input.into_iter().map(|value| JsValue::String(value.into())).collect();
    let max_tokens = positive_or(
        value
            .get("top_provider")
            .and_then(|value| value.get("max_completion_tokens")),
    );
    // Direct native numbers keep Infinity and signed zero until JSON.stringify.
    let fields = [
        ("id", JsValue::String(id.clone())),
        (
            "name",
            JsValue::String(value.get("name").and_then(JsValue::as_string).unwrap_or(id).clone()),
        ),
        ("api", JsValue::String("openai-completions".into())),
        ("baseUrl", JsValue::String(BASE_URL.into())),
        ("provider", JsValue::String("openrouter".into())),
        ("reasoning", JsValue::Bool(supports("reasoning"))),
        ("input", JsValue::Array(input)),
        ("cost", map_pricing(value.get("pricing"))),
        (
            "contextWindow",
            JsValue::Number(positive_or(value.get("context_length"))),
        ),
        ("maxTokens", JsValue::Number(max_tokens)),
    ];
    Some(JsValue::Object(
        fields.into_iter().map(|(key, value)| (key.into(), value)).collect(),
    ))
}

fn map_pricing(pricing: Option<&JsValue>) -> JsValue {
    let fields = [
        ("input", "prompt"),
        ("output", "completion"),
        ("cacheRead", "input_cache_read"),
        ("cacheWrite", "input_cache_write"),
    ];
    JsValue::Object(
        fields
            .into_iter()
            .map(|(target, source)| {
                let cost = parse_cost(pricing.and_then(|value| value.get(source)));
                (target.into(), JsValue::Number(cost))
            })
            .collect(),
    )
}

struct CatalogRequestError {
    error: AbortReason,
    retryable: bool,
    retry_after_ms: f64,
}

impl CatalogRequestError {
    fn new(error: NativeNodeError, retryable: bool, retry_after_ms: f64) -> Self {
        Self {
            error: error
                .with_property("retryable", JsValue::Bool(retryable))
                .with_property("retryAfterMs", JsValue::Number(retry_after_ms))
                .into(),
            retryable,
            retry_after_ms,
        }
    }

    fn terminal(error: AbortReason) -> Self {
        Self {
            error,
            retryable: false,
            retry_after_ms: 0.0,
        }
    }
}

async fn fetch_catalog(
    fetch: &dyn Fetch,
    signal: Option<AbortSignal>,
    now: &(dyn Fn() -> f64 + Sync),
) -> Result<Vec<JsValue>, CatalogRequestError> {
    let response = fetch
        .fetch(
            HttpRequest {
                method: "GET".into(),
                url: format!("{BASE_URL}/models"),
                headers: vec![],
                body: None,
            },
            signal,
        )
        .await
        .map_err(wrap_catalog_transport_failure)?;
    if !(200..300).contains(&response.status) {
        return Err(parse_catalog_http_failure(&response, now));
    }
    let body = read_body_text(response.body)
        .await
        .map_err(|error| CatalogRequestError::terminal(error.thrown_value()))?;
    parse_catalog_payload(&body)
}

fn wrap_catalog_transport_failure(error: CallbackError) -> CatalogRequestError {
    let reason = error.thrown_value();
    let retryable = matches!(&reason,
        AbortReason::Error(error) | AbortReason::DomException(error)
        if error.is_instance_of(NativeErrorClass::TypeError) && error.message == "fetch failed"
    );
    CatalogRequestError::new(
        catalog_error("OpenRouter model catalog request failed").with_cause(reason),
        retryable,
        0.0,
    )
}

fn parse_catalog_http_failure(response: &HttpResponse, now: &(dyn Fn() -> f64 + Sync)) -> CatalogRequestError {
    let values: Vec<_> = response
        .headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("retry-after"))
        .map(|(_, value)| value.trim_matches([' ', '\t', '\r', '\n']))
        .collect();
    let retry_after_ms = parse_retry_after(&values.join(", "), now);
    CatalogRequestError::new(
        catalog_error(format!(
            "OpenRouter model catalog request failed with HTTP {}",
            response.status
        )),
        response.status == 429 || response.status >= 500,
        retry_after_ms,
    )
}

fn parse_catalog_payload(body: &str) -> Result<Vec<JsValue>, CatalogRequestError> {
    let payload = js_json::parse_lossless(body).map_err(|error| {
        CatalogRequestError::terminal(
            NativeNodeError::from_constructor(NativeErrorClass::SyntaxError, error.to_string()).into(),
        )
    })?;
    let data = payload.get("data").and_then(JsValue::as_array).ok_or_else(|| {
        CatalogRequestError::terminal(catalog_error("OpenRouter model catalog response has no data array").into())
    })?;
    let models: Vec<_> = data.iter().filter_map(map_model).collect();
    if models.is_empty() {
        return Err(CatalogRequestError::terminal(
            catalog_error("OpenRouter model catalog contains no tool-capable models").into(),
        ));
    }
    Ok(models)
}

/// Unretried public fetch helper; caller's abort signal is passed unchanged.
pub async fn fetch_openrouter_catalog(
    fetch: &dyn Fetch,
    signal: Option<AbortSignal>,
) -> Result<Vec<JsValue>, AbortReason> {
    fetch_catalog(fetch, signal, &js_now_millis)
        .await
        .map_err(|error| error.error)
}

fn parse_retry_after(value: &str, now: &(dyn Fn() -> f64 + Sync)) -> f64 {
    if value.is_empty() {
        return 0.0;
    }
    let milliseconds = if value.bytes().all(|byte| byte.is_ascii_digit()) {
        value.parse::<f64>().unwrap_or(f64::INFINITY) * 1000.0
    } else {
        date_parse_utf16(JsString::from(value).units()) - now()
    };
    if milliseconds.is_finite() {
        milliseconds.max(0.0)
    } else {
        0.0
    }
}

struct CatalogRetryRuntime<'a> {
    now: &'a (dyn Fn() -> f64 + Sync),
    random: &'a (dyn Fn() -> f64 + Sync),
}

struct CatalogAbortTimer(tokio::task::JoinHandle<()>);
impl CatalogAbortTimer {
    fn start(controller: pi_js_compat::abort::AbortController) -> Self {
        let timeout = tokio::time::sleep(Duration::from_millis(FETCH_TIMEOUT_MS));
        Self(tokio::spawn(async move {
            timeout.await;
            controller.abort(None);
        }))
    }
}
impl Drop for CatalogAbortTimer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn fetch_catalog_with_retries(fetch: &dyn Fetch) -> Result<Vec<JsValue>, NativeNodeError> {
    fetch_catalog_with_runtime(
        fetch,
        CatalogRetryRuntime {
            now: &js_now_millis,
            random: &generate_catalog_jitter,
        },
    )
    .await
}

async fn fetch_catalog_with_runtime(
    fetch: &dyn Fetch,
    runtime: CatalogRetryRuntime<'_>,
) -> Result<Vec<JsValue>, NativeNodeError> {
    let controller = pi_js_compat::abort::AbortController::new();
    let signal = controller.signal();
    let deadline = (runtime.now)() + FETCH_TIMEOUT_MS as f64;
    let _abort_timer = CatalogAbortTimer::start(controller);
    match run_catalog_attempts(fetch, &signal, deadline, &runtime).await {
        Ok(models) => Ok(models),
        Err(reason) if signal.aborted() => {
            Err(catalog_error("OpenRouter model catalog refresh failed: timed out after 5000 ms").with_cause(reason))
        }
        Err(reason) => Err(wrap_error("OpenRouter model catalog refresh failed", reason)),
    }
}

async fn run_catalog_attempts(
    fetch: &dyn Fetch,
    signal: &AbortSignal,
    deadline: f64,
    runtime: &CatalogRetryRuntime<'_>,
) -> Result<Vec<JsValue>, AbortReason> {
    for attempt in 0..3 {
        let error = match fetch_catalog(fetch, Some(signal.clone()), runtime.now).await {
            Ok(models) => return Ok(models),
            Err(error) => error,
        };
        if !error.retryable || attempt == 2 || signal.aborted() {
            return Err(error.error);
        }
        let wait_ms = retry_wait_ms(error.retry_after_ms, attempt, (runtime.random)());
        if (runtime.now)() + wait_ms >= deadline {
            return Err(error.error);
        }
        delay_catalog_retry(wait_ms, signal).await?;
    }
    unreachable!("final attempt returns")
}

fn generate_catalog_jitter() -> f64 {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).expect("OS randomness available");
    (u64::from_ne_bytes(bytes) >> 11) as f64 / ((1_u64 << 53) as f64)
}

fn retry_wait_ms(retry_after_ms: f64, attempt: u32, jitter: f64) -> f64 {
    retry_after_ms.max(250.0 * 2_f64.powi(attempt as i32) * (1.0 + jitter))
}

async fn delay_catalog_retry(wait_ms: f64, signal: &AbortSignal) -> Result<(), AbortReason> {
    let delay_ms = pi_js_compat::node_timers::normalize_timeout_delay(wait_ms) as u64;
    tokio::select! {
        biased;
        reason = signal.wait() => Err(NativeNodeError::new("AbortError", "The operation was aborted")
            .with_cause(reason).with_property("code", JsValue::String("ABORT_ERR".into())).into()),
        _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => Ok(()),
    }
}

fn catalog_error(message: impl Into<JsString>) -> NativeNodeError {
    NativeNodeError::new("Error", message)
}

fn describe_error(reason: &AbortReason) -> Result<JsString, NativeNodeError> {
    let error = match reason {
        AbortReason::Value(value) => {
            return js_json::to_js_string(value)
                .map_err(|message| NativeNodeError::from_constructor(NativeErrorClass::TypeError, message));
        }
        AbortReason::Error(error) | AbortReason::DomException(error) => error,
    };
    match error.cause() {
        Some(cause) if reason_is_truthy(cause) => Ok(pi_js_compat::js_format!(
            "{}: {}",
            error.message,
            describe_error(cause)?
        )),
        _ => Ok(error.message.clone()),
    }
}

fn reason_is_truthy(reason: &AbortReason) -> bool {
    match reason {
        AbortReason::Value(value) => js_json::is_truthy(value),
        AbortReason::Error(_) | AbortReason::DomException(_) => true,
    }
}

fn wrap_error(prefix: &str, reason: AbortReason) -> NativeNodeError {
    match describe_error(&reason) {
        Ok(message) => catalog_error(pi_js_compat::js_format!("{}: {}", prefix, message)).with_cause(reason),
        Err(coercion) => coercion,
    }
}

async fn write_cached_catalog(path: &Path, now: f64, models: &[JsValue]) -> Result<(), NativeNodeError> {
    let fetched_at = pi_js_compat::date::to_iso_string_value(now)
        .map_err(|error| NativeNodeError::from_constructor(NativeErrorClass::RangeError, error.to_string()))?;
    let parent = path.parent().expect("catalog path has parent");
    let temporary = parent.join(format!(
        ".openrouter-{}-{}.tmp",
        std::process::id(),
        pi_ai::utils::random_uuid::random_uuid()?
    ));
    let mut catalog = pi_js_compat::js_json::JsObject::new();
    catalog.insert("fetchedAt".into(), JsValue::String(fetched_at.into()));
    catalog.insert("models".into(), JsValue::Array(models.to_vec()));
    let bytes = format!("{}\n", js_json::stringify_lossless_pretty(&JsValue::Object(catalog)));
    let result = async {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| fs_error(&error, "mkdir", parent))?;
        tokio::fs::write(&temporary, bytes)
            .await
            .map_err(|error| fs_error(&error, "open", &temporary))?;
        tokio::fs::rename(&temporary, path).await.map_err(|error| {
            let source = JsString::from(temporary.display().to_string());
            let destination = JsString::from(path.display().to_string());
            let mut native = fs_error(&error, "rename", &temporary);
            native.message = pi_js_compat::node_fs::node_fs_dest_error(&error, "rename", &source, &destination);
            native.with_property("dest", JsValue::String(destination))
        })?;
        Ok::<_, NativeNodeError>(())
    }
    .await;
    if let Err(primary) = result {
        let mut wrapped = catalog_error(pi_js_compat::js_format!(
            "Cannot write OpenRouter model catalog cache {}: {}",
            path.display().to_string(),
            primary.message
        ));
        wrapped = wrapped.with_cause(primary.clone().into());
        if let Err(cleanup) = tokio::fs::remove_file(&temporary).await
            && cleanup.kind() != std::io::ErrorKind::NotFound
        {
            let cleanup = fs_error(&cleanup, "unlink", &temporary);
            if cleanup.message != primary.message {
                wrapped.message = pi_js_compat::js_format!(
                    "{}; cannot remove temporary cache {}: {}",
                    wrapped.message,
                    temporary.display().to_string(),
                    cleanup.message
                );
            }
        }
        return Err(wrapped);
    }
    Ok(())
}

fn fs_error(error: &std::io::Error, operation: &str, path: &Path) -> NativeNodeError {
    NativeNodeError::from(pi_js_compat::node_fs::node_fs_error_record(
        error,
        operation,
        &JsString::from(path.display().to_string()),
    ))
}

#[cfg(test)]
mod retry_runtime_tests {
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/catalog_retry_runtime.rs"
    ));
}
