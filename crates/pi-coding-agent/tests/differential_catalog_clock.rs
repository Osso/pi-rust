#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_coding_agent::core::model_catalog_cache::{EnsureModelCatalogOptions, ModelCatalogCache};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Body(Option<Vec<u8>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { self.0.take().map(Ok) })
    }
}
struct CatalogFetch(Value);
impl Fetch for CatalogFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        assert_eq!(request.url, "https://openrouter.ai/api/v1/models");
        assert!(signal.is_some());
        let body = serde_json::to_vec(&self.0).unwrap();
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                status_text: "".into(),
                headers: vec![],
                body: Box::new(Body(Some(body))),
            })
        })
    }
}
fn payload() -> Value {
    json!({"data":[{"id":"q15-clock","supported_parameters":["tools"]}]})
}

#[tokio::test]
async fn cold_unforced_refresh_persists_completion_time_after_freshness_probe() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pi/models/openrouter.json");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "catalog-startup-oracle.mts",
        "packages/coding-agent/src",
        &[("XDG_CACHE_HOME", temp.path().as_os_str().to_owned())],
    );
    let now = 1_766_000_000_000.0;
    let expected = source
        .request(&json!({"id":1,"operation":"ensure_clock","force":false,"now":now,"tickMs":1000,"payload":payload()}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert!(expected["output"].get("error").is_none(), "{expected}");
    let expected_bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    ModelCatalogCache::default()
        .ensure_fresh(
            &path,
            EnsureModelCatalogOptions {
                force: false,
                fetch: Arc::new(CatalogFetch(payload())),
                now: Arc::new(move || now + observed.fetch_add(1, Ordering::SeqCst) as f64 * 1000.0),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(path).unwrap(),
        expected_bytes,
        "cache timestamp must use completion time, not the freshness-probe value"
    );
}

#[tokio::test]
async fn invalid_completion_date_rejects_before_writing_the_cache() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pi/models/openrouter.json");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "catalog-startup-oracle.mts",
        "packages/coding-agent/src",
        &[("XDG_CACHE_HOME", temp.path().as_os_str().to_owned())],
    );
    let expected =
        source.request(&json!({"id":1,"operation":"ensure_clock","force":true,"invalidDate":true,"payload":payload()}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert!(!path.exists());
    let result = ModelCatalogCache::default()
        .ensure_fresh(
            &path,
            EnsureModelCatalogOptions {
                force: true,
                fetch: Arc::new(CatalogFetch(payload())),
                now: Arc::new(|| f64::NAN),
            },
        )
        .await;
    let Err(error) = result else {
        panic!("invalid Date unexpectedly succeeded and wrote a cache");
    };
    assert_eq!(
        json!({"name":error.name,"message":error.message}),
        expected["output"]["error"]
    );
    assert!(!path.exists(), "invalid date must not create a cache or publish models");
}
