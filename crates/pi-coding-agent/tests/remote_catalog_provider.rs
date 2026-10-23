//! Behavioral tests for `core/remote-catalog-provider.ts` with an injected `Fetch`.

use std::sync::{Arc, Mutex};

use pi_ai::auth::types::ProviderAuth;
use pi_ai::models::{ModelsPublication, PersistAction, Provider, RefreshModelsContext};
use pi_ai::models_store::ModelsStoreEntry;
use pi_ai::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, Fetch, HttpBody, HttpRequest, HttpResponse,
    SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{Context, DeferredHandle, Model};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_coding_agent::core::remote_catalog_provider::{REMOTE_CATALOG_REFRESH_INTERVAL_MS, with_remote_catalog};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};

type StubResponse = (u16, Vec<(String, String)>, String);

struct StubBody(Vec<u8>);

impl HttpBody for StubBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = std::mem::take(&mut self.0);
        Box::pin(async move { (!chunk.is_empty()).then_some(Ok(chunk)) })
    }
}

#[derive(Default)]
struct StubFetch {
    responses: Mutex<Vec<HttpResponse>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl StubFetch {
    fn with_responses(responses: Vec<StubResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(
                responses
                    .into_iter()
                    .map(|(status, headers, body)| HttpResponse {
                        status,
                        status_text: String::new(),
                        headers,
                        body: Box::new(StubBody(body.into_bytes())),
                    })
                    .collect(),
            ),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Fetch for StubFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        let response = self.responses.lock().unwrap().remove(0);
        Box::pin(async move { Ok(response) })
    }
}

struct StaticProvider {
    id: String,
    models: Vec<Model>,
}

impl Provider for StaticProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "Static"
    }

    fn auth(&self) -> &ProviderAuth {
        static AUTH: std::sync::OnceLock<ProviderAuth> = std::sync::OnceLock::new();
        AUTH.get_or_init(|| ProviderAuth {
            api_key: None,
            oauth: None,
        })
    }

    fn get_models(&self) -> Vec<Model> {
        self.models.clone()
    }

    fn stream(&self, model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
        pi_ai::request::error_stream(&model, "not used")
    }

    fn stream_simple(
        &self,
        model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        pi_ai::request::error_stream(&model, "not used")
    }

    fn supports_fetch_deferred(&self) -> bool {
        false
    }

    fn fetch_deferred(
        &self,
        model: Model,
        _handle: DeferredHandle,
        _options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        pi_ai::request::error_stream(&model, "not used")
    }

    fn supports_cancel_deferred(&self) -> bool {
        false
    }

    fn cancel_deferred(
        &self,
        _model: Model,
        _handle: DeferredHandle,
        _options: DeferredCancelOptions,
    ) -> BoxFuture<'static, Result<(), CallbackError>> {
        Box::pin(async { Err(CallbackError::new("not used")) })
    }
}

fn model(id: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "api": "openai-completions",
        "provider": "test",
        "baseUrl": "https://example.test/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 1000,
        "maxTokens": 100,
    }))
    .unwrap()
}

fn refresh_context(
    stored: Option<ModelsStoreEntry>,
    allow_network: bool,
    force: bool,
    publications: Arc<Mutex<Vec<Value>>>,
) -> RefreshModelsContext {
    let publish: pi_ai::models::PublishFn = Arc::new(move |publication: ModelsPublication| {
        let publications = Arc::clone(&publications);
        Box::pin(async move {
            let mut entry = json!({"kind": match &publication.persist {
                PersistAction::Unchanged => "unchanged",
                PersistAction::Delete => "delete",
                PersistAction::Write(_) => "write",
            }});
            if let PersistAction::Write(stored) = &publication.persist {
                entry["entry"] = serde_json::to_value(stored).unwrap();
            }
            publications.lock().unwrap().push(entry);
            if let Some(update) = &publication.update {
                update();
            }
            Ok(true)
        })
    });
    RefreshModelsContext {
        credential: None,
        stored,
        publish,
        allow_network,
        force: Some(force),
        signal: AbortSignal::default(),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn catalog_provider(fetch: Arc<StubFetch>) -> Arc<dyn Provider> {
    with_remote_catalog(
        Arc::new(StaticProvider {
            id: "test".to_owned(),
            models: vec![model("baseline")],
        }),
        fetch,
        Some("https://catalog.test"),
        None,
    )
}

#[test]
fn restores_the_stored_overlay_and_merges_by_id() {
    let fetch = StubFetch::with_responses(Vec::new());
    let provider = catalog_provider(fetch.clone());
    let publications = Arc::new(Mutex::new(Vec::new()));
    let stored = ModelsStoreEntry {
        models: vec![model("baseline"), model("dynamic")],
        last_modified: Some(10.0),
        checked_at: Some(now() - REMOTE_CATALOG_REFRESH_INTERVAL_MS - 1.0),
        etag: Some("\"v1\"".to_owned()),
    };
    let context = refresh_context(Some(stored), false, false, Arc::clone(&publications));
    runtime()
        .block_on(async { provider.refresh_models(context).unwrap().await })
        .unwrap();

    let ids: Vec<String> = provider.get_models().into_iter().map(|model| model.id).collect();
    assert_eq!(ids, ["baseline", "dynamic"]);
    let published = publications.lock().unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0]["kind"], "unchanged");
}

#[test]
fn a_newer_local_generation_suppresses_the_overlay() {
    let fetch = StubFetch::with_responses(Vec::new());
    let provider = with_remote_catalog(
        Arc::new(StaticProvider {
            id: "test".to_owned(),
            models: vec![model("baseline")],
        }),
        fetch,
        Some("https://catalog.test"),
        Some(100.0),
    );
    let publications = Arc::new(Mutex::new(Vec::new()));
    let stored = ModelsStoreEntry {
        models: vec![model("dynamic")],
        last_modified: Some(50.0),
        checked_at: None,
        etag: None,
    };
    let context = refresh_context(Some(stored), false, false, Arc::clone(&publications));
    runtime()
        .block_on(async { provider.refresh_models(context).unwrap().await })
        .unwrap();
    let ids: Vec<String> = provider.get_models().into_iter().map(|model| model.id).collect();
    assert_eq!(ids, ["baseline"]);
}

#[test]
fn offline_and_fresh_catalogs_skip_the_network() {
    let fetch = StubFetch::with_responses(Vec::new());
    let provider = catalog_provider(fetch.clone());
    let publications = Arc::new(Mutex::new(Vec::new()));

    let context = refresh_context(None, false, false, Arc::clone(&publications));
    runtime()
        .block_on(async { provider.refresh_models(context).unwrap().await })
        .unwrap();
    assert!(fetch.requests().is_empty());

    let fresh = ModelsStoreEntry {
        models: vec![model("cached")],
        last_modified: Some(5.0),
        checked_at: Some(now()),
        etag: Some("\"v1\"".to_owned()),
    };
    let context = refresh_context(Some(fresh), true, false, Arc::clone(&publications));
    runtime()
        .block_on(async { provider.refresh_models(context).unwrap().await })
        .unwrap();
    assert!(fetch.requests().is_empty(), "fresh catalogs are not revalidated");
}

#[test]
fn forced_refresh_revalidates_with_the_stored_etag() {
    let fetch = StubFetch::with_responses(vec![(304, vec![], String::new())]);
    let provider = catalog_provider(fetch.clone());
    let publications = Arc::new(Mutex::new(Vec::new()));
    let stored = ModelsStoreEntry {
        models: vec![model("cached")],
        last_modified: Some(5.0),
        checked_at: Some(now()),
        etag: Some("\"v1\"".to_owned()),
    };
    let context = refresh_context(Some(stored), true, true, Arc::clone(&publications));
    runtime()
        .block_on(async { provider.refresh_models(context).unwrap().await })
        .unwrap();

    let requests = fetch.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "https://catalog.test/api/models/providers/test");
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(name, value)| name == "if-none-match" && value == "\"v1\"")
    );
    let published = publications.lock().unwrap();
    assert_eq!(published.last().unwrap()["kind"], "write");
    assert_eq!(published.last().unwrap()["entry"]["etag"], "\"v1\"");
    assert!(published.last().unwrap()["entry"]["checkedAt"].as_f64().unwrap() > now() - 5.0);
}

#[test]
fn missing_and_unsupported_catalogs_persist_a_marker() {
    for status in [404, 501] {
        let fetch = StubFetch::with_responses(vec![(status, vec![], String::new())]);
        let provider = catalog_provider(fetch);
        let publications = Arc::new(Mutex::new(Vec::new()));
        let stored = ModelsStoreEntry {
            models: vec![model("cached")],
            last_modified: Some(5.0),
            checked_at: Some(now() - REMOTE_CATALOG_REFRESH_INTERVAL_MS - 1.0),
            etag: Some("\"v1\"".to_owned()),
        };
        let context = refresh_context(Some(stored), true, false, Arc::clone(&publications));
        runtime()
            .block_on(async { provider.refresh_models(context).unwrap().await })
            .unwrap();
        let published = publications.lock().unwrap();
        let entry = &published.last().unwrap()["entry"];
        assert_eq!(entry["lastModified"], 0);
        assert_eq!(entry["etag"], Value::Null);
        assert!(entry["checkedAt"].as_f64().unwrap() > 0.0);
    }
}

#[test]
fn transient_failures_keep_the_cached_catalog_and_report_the_status() {
    // `fetchWithRetry` retries 5xx responses twice before failing.
    let fetch = StubFetch::with_responses(vec![
        (500, vec![], String::new()),
        (500, vec![], String::new()),
        (500, vec![], String::new()),
    ]);
    let provider = catalog_provider(fetch);
    let publications = Arc::new(Mutex::new(Vec::new()));
    let stored = ModelsStoreEntry {
        models: vec![model("cached")],
        last_modified: Some(5.0),
        checked_at: Some(now() - REMOTE_CATALOG_REFRESH_INTERVAL_MS - 1.0),
        etag: Some("\"v1\"".to_owned()),
    };
    let context = refresh_context(Some(stored), true, false, Arc::clone(&publications));
    let error = runtime()
        .block_on(async { provider.refresh_models(context).unwrap().await })
        .unwrap_err();
    assert_eq!(error.to_string(), "Model catalog request failed for test: 500");
    let published = publications.lock().unwrap();
    let entry = &published.last().unwrap()["entry"];
    assert_eq!(entry["etag"], "\"v1\"", "the validator survives transient failures");
    assert_eq!(entry["models"][0]["id"], "cached");
}

#[test]
fn a_valid_catalog_is_persisted_with_provider_and_last_modified() {
    let body = serde_json::to_string(&json!([
        {"id": "one", "name": "One", "api": "openai-completions", "provider": "wrong", "baseUrl": "https://example.test/v1",
         "reasoning": false, "input": ["text"],
         "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}, "contextWindow": 1, "maxTokens": 1},
        {"id": "two", "name": "Two", "api": "openai-completions", "baseUrl": "https://example.test/v1",
         "reasoning": false, "input": ["text"],
         "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}, "contextWindow": 1, "maxTokens": 1},
        {"name": "no id"}
    ]))
    .unwrap();
    let fetch = StubFetch::with_responses(vec![(
        200,
        vec![
            ("last-modified".to_owned(), "Wed, 21 Oct 2015 07:28:00 GMT".to_owned()),
            ("etag".to_owned(), "\"v2\"".to_owned()),
        ],
        body,
    )]);
    let provider = catalog_provider(fetch);
    let publications = Arc::new(Mutex::new(Vec::new()));
    let context = refresh_context(None, true, false, Arc::clone(&publications));
    runtime()
        .block_on(async { provider.refresh_models(context).unwrap().await })
        .unwrap();

    let published = publications.lock().unwrap();
    let entry = &published.last().unwrap()["entry"];
    assert_eq!(entry["etag"], "\"v2\"");
    assert_eq!(entry["lastModified"], 1_445_412_480_000.0_f64 as i64);
    assert_eq!(entry["models"].as_array().unwrap().len(), 2);
    assert!(
        entry["models"]
            .as_array()
            .unwrap()
            .iter()
            .all(|model| model["provider"] == "test")
    );
    let ids: Vec<String> = provider.get_models().into_iter().map(|model| model.id).collect();
    assert_eq!(ids, ["baseline", "one", "two"]);
}

#[test]
fn invalid_catalog_shapes_report_the_provider() {
    for body in ["\"nope\"", "42", "null"] {
        let fetch = StubFetch::with_responses(vec![(200, vec![], body.to_owned())]);
        let provider = catalog_provider(fetch);
        let publications = Arc::new(Mutex::new(Vec::new()));
        let context = refresh_context(None, true, false, Arc::clone(&publications));
        let error = runtime()
            .block_on(async { provider.refresh_models(context).unwrap().await })
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Invalid model catalog for provider \"test\"",
            "body {body}"
        );
    }
}

#[test]
fn an_aborted_signal_stops_before_fetching() {
    let fetch = StubFetch::with_responses(vec![(200, vec![], "[]".to_owned())]);
    let provider = catalog_provider(fetch.clone());
    let publications = Arc::new(Mutex::new(Vec::new()));
    let context = {
        let mut context = refresh_context(None, true, true, publications);
        context.signal = AbortSignal::abort(None);
        context
    };
    runtime()
        .block_on(async { provider.refresh_models(context).unwrap().await })
        .unwrap();
    assert!(fetch.requests().is_empty());
}

fn now() -> f64 {
    pi_ai::request::now_millis() as f64
}
