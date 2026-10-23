//! Frozen `core/model-catalog-cache.ts`: OpenRouter's startup cache and explicit refresh.
//! Raw catalog objects retain unknown properties and UTF-16 until a typed consumer boundary.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pi_ai::request::{CallbackError, Fetch, HttpRequest, read_body_text};
use pi_js_compat::date::{date_parse_utf16, js_now_millis};
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

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
    path: PathBuf,
    bundled: Vec<JsValue>,
    memoized: Mutex<Option<Vec<JsValue>>>,
}

impl ModelCatalogCache {
    pub fn new(path: PathBuf) -> Self {
        let bundled = pi_ai::compat::get_models("openrouter")
            .into_iter()
            .map(|model| {
                js_json::parse_lossless(serde_json::to_string(&model).expect("catalog model serializes"))
                    .expect("serialized model is JSON")
            })
            .collect();
        Self {
            path,
            bundled,
            memoized: Mutex::new(None),
        }
    }

    pub fn get_models(&self) -> Vec<JsValue> {
        self.memoized
            .lock()
            .expect("catalog lock poisoned")
            .clone()
            .unwrap_or_else(|| self.bundled.clone())
    }

    pub async fn load_at_startup(&self, now: f64) -> ModelCatalogRefreshResult {
        let cached = read_cached_catalog(&self.path).await;
        let count = cached.as_ref().map_or(0, |catalog| catalog.models.len());
        if let Some(cached) = cached.filter(|cached| is_fresh(cached, now)) {
            self.build_result(cached.models, "cache", 0, count)
        } else {
            self.build_result(Vec::new(), "bundled", 0, count)
        }
    }

    pub async fn ensure_fresh(
        &self,
        options: EnsureModelCatalogOptions,
    ) -> Result<ModelCatalogRefreshResult, CallbackError> {
        let cached = read_cached_catalog(&self.path).await;
        if !options.force
            && cached
                .as_ref()
                .is_some_and(|catalog| is_fresh(catalog, (options.now)()))
        {
            let cached = cached.expect("fresh cache exists");
            let count = cached.models.len();
            return Ok(self.build_result(cached.models, "cache", 0, count));
        }
        let models = fetch_catalog_with_retries(options.fetch.as_ref()).await?;
        let count = models.len();
        write_cached_catalog(&self.path, (options.now)(), &models).await?;
        Ok(self.build_result(
            models,
            "network",
            count,
            cached.as_ref().map_or(0, |cached| cached.models.len()),
        ))
    }

    fn build_result(
        &self,
        refreshed: Vec<JsValue>,
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
            cache_path: self.path.clone(),
        }
    }
}

fn is_fresh(cached: &CachedCatalog, now: f64) -> bool {
    now - date_parse_utf16(cached.fetched_at.units()) < MAX_AGE_MS
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
    let pricing = value.get("pricing");
    let cost = |key| parse_cost(pricing.and_then(|value| value.get(key)));
    let mut model = JsValue::from(serde_json::json!({
        "api":"openai-completions", "baseUrl":BASE_URL, "provider":"openrouter", "reasoning":supports("reasoning"),
        "input":if image { vec!["text", "image"] } else { vec!["text"] },
        "cost":{"input":cost("prompt"),"output":cost("completion"),"cacheRead":cost("input_cache_read"),"cacheWrite":cost("input_cache_write")},
        "contextWindow":positive_or(value.get("context_length")),
        "maxTokens":positive_or(value.get("top_provider").and_then(|value| value.get("max_completion_tokens")))
    }));
    let object = model.as_object_mut().expect("model is object");
    // Insertion order is externally visible in the persisted cache.
    let mut ordered = pi_js_compat::js_json::JsObject::new();
    ordered.insert("id".into(), JsValue::String(id.clone()));
    ordered.insert(
        "name".into(),
        JsValue::String(value.get("name").and_then(JsValue::as_string).unwrap_or(id).clone()),
    );
    for key in [
        "api",
        "baseUrl",
        "provider",
        "reasoning",
        "input",
        "cost",
        "contextWindow",
        "maxTokens",
    ] {
        ordered.insert(key.into(), object.get(key).expect("mapped property").clone());
    }
    Some(JsValue::Object(ordered))
}

struct CatalogRequestError {
    error: CallbackError,
    retryable: bool,
    retry_after_ms: f64,
}

impl CatalogRequestError {
    fn terminal(error: CallbackError) -> Self {
        Self {
            error,
            retryable: false,
            retry_after_ms: 0.0,
        }
    }
}

async fn fetch_catalog(
    fetch: &dyn Fetch,
    signal: Option<CancellationToken>,
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
        .map_err(|error| {
            let retryable = error.name == "TypeError" && error.message == "fetch failed";
            CatalogRequestError {
                error: wrap_error("OpenRouter model catalog request failed", error),
                retryable,
                retry_after_ms: 0.0,
            }
        })?;
    if !(200..300).contains(&response.status) {
        let retry_after_ms = response
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("retry-after"))
            .map_or(0.0, |(_, value)| parse_retry_after(value));
        return Err(CatalogRequestError {
            error: CallbackError::new(format!(
                "OpenRouter model catalog request failed with HTTP {}",
                response.status
            )),
            retryable: response.status == 429 || response.status >= 500,
            retry_after_ms,
        });
    }
    let body = read_body_text(response.body)
        .await
        .map_err(CatalogRequestError::terminal)?;
    let payload = js_json::parse_lossless(body)
        .map_err(|error| CatalogRequestError::terminal(CallbackError::new(error.to_string())))?;
    let data = payload.get("data").and_then(JsValue::as_array).ok_or_else(|| {
        CatalogRequestError::terminal(CallbackError::new(
            "OpenRouter model catalog response has no data array",
        ))
    })?;
    let models: Vec<_> = data.iter().filter_map(map_model).collect();
    if models.is_empty() {
        return Err(CatalogRequestError::terminal(CallbackError::new(
            "OpenRouter model catalog contains no tool-capable models",
        )));
    }
    Ok(models)
}

/// Unretried public fetch helper; caller's abort signal is passed unchanged.
pub async fn fetch_openrouter_catalog(
    fetch: &dyn Fetch,
    signal: Option<CancellationToken>,
) -> Result<Vec<JsValue>, CallbackError> {
    fetch_catalog(fetch, signal).await.map_err(|error| error.error)
}

fn parse_retry_after(value: &str) -> f64 {
    if value.is_empty() {
        return 0.0;
    }
    let milliseconds = if value.bytes().all(|byte| byte.is_ascii_digit()) {
        value.parse::<f64>().unwrap_or(f64::INFINITY) * 1000.0
    } else {
        date_parse_utf16(JsString::from(value).units()) - js_now_millis()
    };
    if milliseconds.is_finite() {
        milliseconds.max(0.0)
    } else {
        0.0
    }
}

async fn fetch_catalog_with_retries(fetch: &dyn Fetch) -> Result<Vec<JsValue>, CallbackError> {
    let signal = CancellationToken::new();
    let deadline = Instant::now() + Duration::from_millis(FETCH_TIMEOUT_MS);
    let operation = async {
        for attempt in 0..3 {
            match fetch_catalog(fetch, Some(signal.clone())).await {
                Ok(models) => return Ok(models),
                Err(error) => {
                    if !error.retryable || attempt == 2 {
                        return Err(error.error);
                    }
                    let mut bytes = [0; 8];
                    getrandom::fill(&mut bytes).expect("OS randomness available");
                    let jitter = (u64::from_ne_bytes(bytes) >> 11) as f64 / ((1_u64 << 53) as f64);
                    let wait_ms = error.retry_after_ms.max(250.0 * 2_f64.powi(attempt) * (1.0 + jitter));
                    if wait_ms >= deadline.saturating_duration_since(Instant::now()).as_secs_f64() * 1000.0 {
                        return Err(error.error);
                    }
                    tokio::time::sleep(Duration::from_secs_f64(wait_ms / 1000.0)).await;
                }
            }
        }
        unreachable!("final attempt returns")
    };
    match tokio::time::timeout_at(deadline.into(), operation).await {
        Ok(result) => result.map_err(|error| wrap_error("OpenRouter model catalog refresh failed", error)),
        Err(_) => {
            signal.cancel();
            Err(CallbackError::new(
                "OpenRouter model catalog refresh failed: timed out after 5000 ms",
            ))
        }
    }
}

fn wrap_error(prefix: &str, error: CallbackError) -> CallbackError {
    let description = error.cause.as_ref().map_or_else(
        || error.message.clone(),
        |cause| pi_js_compat::js_format!("{}: {}", error.message, cause),
    );
    let mut wrapped = CallbackError::new(pi_js_compat::js_format!("{}: {}", prefix, description));
    wrapped.cause = Some(error.to_string());
    wrapped
}

async fn write_cached_catalog(path: &Path, now: f64, models: &[JsValue]) -> Result<(), CallbackError> {
    let parent = path.parent().expect("catalog path has parent");
    let temporary = parent.join(format!(
        ".openrouter-{}-{}.tmp",
        std::process::id(),
        pi_ai::utils::random_uuid::random_uuid()
    ));
    let mut catalog = pi_js_compat::js_json::JsObject::new();
    catalog.insert(
        "fetchedAt".into(),
        JsValue::String(pi_js_compat::date::to_iso_string(now as i64).into()),
    );
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
            let message = pi_js_compat::node_fs::node_fs_error(
                &error,
                "rename",
                &JsString::from(temporary.display().to_string()),
            );
            CallbackError::new(pi_js_compat::js_format!(
                "{} -> '{}'",
                message,
                path.display().to_string()
            ))
        })?;
        Ok::<_, CallbackError>(())
    }
    .await;
    if let Err(primary) = result {
        let mut wrapped = CallbackError::new(pi_js_compat::js_format!(
            "Cannot write OpenRouter model catalog cache {}: {}",
            path.display().to_string(),
            primary.message
        ));
        wrapped.cause = Some(primary.to_string());
        if let Err(cleanup) = tokio::fs::remove_file(&temporary).await {
            if cleanup.kind() != std::io::ErrorKind::NotFound {
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
        }
        return Err(wrapped);
    }
    Ok(())
}

fn fs_error(error: &std::io::Error, operation: &str, path: &Path) -> CallbackError {
    CallbackError::new(pi_js_compat::node_fs::node_fs_error(
        error,
        operation,
        &JsString::from(path.display().to_string()),
    ))
}
