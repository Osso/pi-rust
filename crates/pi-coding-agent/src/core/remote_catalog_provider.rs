//! Port of `packages/coding-agent/src/core/remote-catalog-provider.ts`.
//!
//! Adds the persisted pi.dev catalog overlay to a static built-in provider. HTTP goes through the
//! injected `Fetch` contract so tests never reach the network.

use std::sync::{Arc, Mutex};

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use pi_ai::auth::types::{Credential, ProviderAuth};
use pi_ai::models::{ModelsPublication, PersistAction, Provider, RefreshModelsContext};
use pi_ai::models_store::ModelsStoreEntry;
use pi_ai::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, Fetch, HeaderTransform, HttpRequest,
    SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{Context, DeferredHandle, Model, ProviderHeaders};
use serde_json::Value;

use crate::config::package_config;
use crate::utils::management_http::{FetchRetryOptions, fetch_with_retry};
use crate::utils::pi_user_agent::get_pi_user_agent;

const DEFAULT_CATALOG_BASE_URL: &str = "https://pi.dev";
/// Catalogs are revalidated at most every four hours unless forced.
pub const REMOTE_CATALOG_REFRESH_INTERVAL_MS: f64 = 4.0 * 60.0 * 60.0 * 1000.0;

/// `encodeURIComponent` leaves these ASCII characters unescaped.
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn merge_models(baseline: Vec<Model>, dynamic: &[Model]) -> Vec<Model> {
    let mut merged = baseline;
    for model in dynamic {
        match merged.iter_mut().find(|entry| entry.id == model.id) {
            Some(entry) => *entry = model.clone(),
            None => merged.push(model.clone()),
        }
    }
    merged
}

fn remote_models(entry: Option<&ModelsStoreEntry>, local_generated_at: Option<f64>) -> Vec<Model> {
    let Some(entry) = entry else {
        return Vec::new();
    };
    if let Some(local_generated_at) = local_generated_at
        && (entry.last_modified.is_none() || entry.last_modified.is_some_and(|value| value <= local_generated_at))
    {
        return Vec::new();
    }
    entry.models.clone()
}

/// `parseCatalog`: an array, `{models: [...]}`, or an object of models.
fn parse_catalog(provider_id: &str, value: &Value) -> Result<Vec<Model>, CallbackError> {
    let entries: Vec<Value> = match value {
        Value::Array(entries) => entries.clone(),
        Value::Object(object) => match object.get("models") {
            Some(Value::Array(entries)) => entries.clone(),
            _ => object.values().cloned().collect(),
        },
        _ => {
            return Err(CallbackError::new(format!(
                "Invalid model catalog for provider \"{provider_id}\""
            )));
        }
    };
    Ok(entries
        .into_iter()
        .filter(|entry| entry.is_object() && entry.get("id").is_some())
        .filter_map(|entry| {
            // `{...model, provider: providerId}`: the catalog may omit or override `provider`.
            let mut object = entry.as_object()?.clone();
            object.insert("provider".to_owned(), Value::String(provider_id.to_owned()));
            serde_json::from_value(Value::Object(object)).ok()
        })
        .collect())
}

/// A static provider with a persisted remote catalog overlay.
pub struct RemoteCatalogProvider {
    inner: Arc<dyn Provider>,
    fetch: Arc<dyn Fetch>,
    catalog_base_url: String,
    local_generated_at: Option<f64>,
    dynamic_models: Arc<Mutex<Vec<Model>>>,
}

/// `withRemoteCatalog`: wraps `provider` with the persisted catalog overlay.
pub fn with_remote_catalog(
    provider: Arc<dyn Provider>,
    fetch: Arc<dyn Fetch>,
    catalog_base_url: Option<&str>,
    local_generated_at: Option<f64>,
) -> Arc<dyn Provider> {
    Arc::new(RemoteCatalogProvider {
        inner: provider,
        fetch,
        catalog_base_url: catalog_base_url.unwrap_or(DEFAULT_CATALOG_BASE_URL).to_owned(),
        local_generated_at,
        dynamic_models: Arc::new(Mutex::new(Vec::new())),
    })
}

impl RemoteCatalogProvider {
    fn catalog_url(&self, provider_id: &str) -> String {
        let encoded = utf8_percent_encode(provider_id, URI_COMPONENT).to_string();
        format!(
            "{}/api/models/providers/{encoded}",
            self.catalog_base_url.trim_end_matches('/')
        )
    }
}

impl Provider for RemoteCatalogProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn base_url(&self) -> Option<&str> {
        self.inner.base_url()
    }

    fn headers(&self) -> Option<&ProviderHeaders> {
        self.inner.headers()
    }

    fn auth(&self) -> &ProviderAuth {
        self.inner.auth()
    }

    fn get_models(&self) -> Vec<Model> {
        merge_models(self.inner.get_models(), &lock(&self.dynamic_models))
    }

    fn supports_refresh_models(&self) -> bool {
        true
    }

    fn refresh_models(&self, context: RefreshModelsContext) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        let provider_id = self.inner.id().to_owned();
        let restored: Vec<Model> = remote_models(context.stored.as_ref(), self.local_generated_at)
            .into_iter()
            .filter(|model| model.provider == provider_id)
            .collect();
        let dynamic_models = Arc::clone(&self.dynamic_models);
        let url = self.catalog_url(&provider_id);
        let fetch = Arc::clone(&self.fetch);
        let local_generated_at = self.local_generated_at;
        let publisher = Arc::clone(&self.dynamic_models);

        Some(Box::pin(async move {
            let published = (context.publish)(ModelsPublication::new(PersistAction::Unchanged).with_update({
                let dynamic_models = Arc::clone(&dynamic_models);
                move || *lock(&dynamic_models) = restored.clone()
            }))
            .await?;
            if !published {
                return Ok(());
            }
            if !context.allow_network || context.signal.is_cancelled() {
                return Ok(());
            }
            let stored = context.stored.clone();
            let checked_at_recent = stored.as_ref().is_some_and(|entry| {
                entry.checked_at.is_some()
                    && entry.last_modified.is_some()
                    && now_ms() - entry.checked_at.unwrap_or_default() < REMOTE_CATALOG_REFRESH_INTERVAL_MS
            });
            if context.force != Some(true) && checked_at_recent {
                return Ok(());
            }

            // Only revalidate when a cached body backs the validator.
            let validator = stored
                .as_ref()
                .filter(|entry| !entry.models.is_empty())
                .and_then(|entry| entry.etag.clone());
            let mut headers = vec![
                ("accept".to_owned(), "application/json".to_owned()),
                ("user-agent".to_owned(), get_pi_user_agent(&package_config().version)),
            ];
            if let Some(validator) = &validator {
                headers.push(("if-none-match".to_owned(), validator.clone()));
            }
            let response = fetch_with_retry(
                fetch.as_ref(),
                HttpRequest {
                    method: "GET".to_owned(),
                    url,
                    headers,
                    body: None,
                },
                Some(context.signal.clone()),
                &FetchRetryOptions::default(),
            )
            .await?;
            if context.signal.is_cancelled() {
                return Ok(());
            }
            let checked_at = now_ms();

            if response.status == 304
                && let Some(stored) = &stored
            {
                let mut entry = stored.clone();
                entry.checked_at = Some(checked_at);
                (context.publish)(ModelsPublication::new(PersistAction::Write(entry))).await?;
                return Ok(());
            }
            if response.status == 404 || response.status == 501 {
                let mut entry = stored.clone().unwrap_or_else(|| ModelsStoreEntry::new(Vec::new()));
                entry.checked_at = Some(checked_at);
                entry.last_modified = Some(0.0);
                entry.etag = None;
                (context.publish)(ModelsPublication::new(PersistAction::Write(entry))).await?;
                return Ok(());
            }
            if !(200..300).contains(&response.status) {
                // Transient failure: keep the cached body and its validator.
                let mut entry = stored.clone().unwrap_or_else(|| ModelsStoreEntry::new(Vec::new()));
                entry.checked_at = Some(checked_at);
                (context.publish)(ModelsPublication::new(PersistAction::Write(entry))).await?;
                return Err(CallbackError::new(format!(
                    "Model catalog request failed for {provider_id}: {}",
                    response.status
                )));
            }

            let last_modified = header_value(&response.headers, "last-modified")
                .and_then(|value| httpdate::parse_http_date(value).ok())
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_millis() as f64)
                .unwrap_or(0.0);
            let etag = header_value(&response.headers, "etag").map(str::to_owned);
            let body = read_body_text(response.body).await?;
            let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let refreshed = parse_catalog(&provider_id, &parsed)?;
            if context.signal.is_cancelled() {
                return Ok(());
            }
            let entry = ModelsStoreEntry {
                models: refreshed,
                last_modified: Some(last_modified),
                checked_at: Some(checked_at),
                etag,
            };
            let published_models = remote_models(Some(&entry), local_generated_at);
            (context.publish)(ModelsPublication::new(PersistAction::Write(entry)).with_update({
                let publisher = Arc::clone(&publisher);
                move || *lock(&publisher) = published_models.clone()
            }))
            .await?;
            Ok(())
        }))
    }

    fn filter_models(&self, models: Vec<Model>, credential: Option<&Credential>) -> Vec<Model> {
        self.inner.filter_models(models, credential)
    }

    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> pi_ai::utils::event_stream::AssistantMessageEventStream {
        self.inner.stream(model, context, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> pi_ai::utils::event_stream::AssistantMessageEventStream {
        self.inner.stream_simple(model, context, options)
    }

    fn supports_fetch_deferred(&self) -> bool {
        self.inner.supports_fetch_deferred()
    }

    fn fetch_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> pi_ai::utils::event_stream::AssistantMessageEventStream {
        self.inner.fetch_deferred(model, handle, options)
    }

    fn supports_cancel_deferred(&self) -> bool {
        self.inner.supports_cancel_deferred()
    }

    fn cancel_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredCancelOptions,
    ) -> BoxFuture<'static, Result<(), CallbackError>> {
        self.inner.cancel_deferred(model, handle, options)
    }
}

fn now_ms() -> f64 {
    pi_ai::request::now_millis() as f64
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

async fn read_body_text(mut body: Box<dyn pi_ai::request::HttpBody>) -> Result<String, CallbackError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Header transform helper kept for symmetry with other provider wrappers.
pub fn passthrough_headers(headers: ProviderHeaders) -> BoxFuture<'static, Result<ProviderHeaders, CallbackError>> {
    Box::pin(async move { Ok(headers) })
}

/// Identity transform, useful when composing wrappers.
pub fn identity_transform() -> HeaderTransform {
    Arc::new(passthrough_headers)
}
