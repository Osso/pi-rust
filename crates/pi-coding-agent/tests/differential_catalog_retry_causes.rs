use crate::shared_oracle as oracle;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_coding_agent::core::model_catalog_cache::{EnsureModelCatalogOptions, ModelCatalogCache};
use pi_js_compat::{
    abort::{AbortReason, AbortSignal},
    js_json::JsValue,
    node_error::{NativeErrorClass, NativeNodeError},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct RejectFetch {
    reason: AbortReason,
    calls: AtomicUsize,
}
impl Fetch for RejectFetch {
    fn fetch(&self, _: HttpRequest, signal: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        assert!(signal.is_some());
        self.calls.fetch_add(1, Ordering::SeqCst);
        let error = CallbackError::from_rejection(self.reason.clone());
        Box::pin(async move { Err(error) })
    }
}

fn materialize_reason(spec: &JsValue) -> AbortReason {
    if spec["kind"].as_string().unwrap() == "value" {
        return AbortReason::Value(spec["value"].clone());
    }
    let class = if spec["constructor"].as_string().unwrap() == "TypeError" {
        NativeErrorClass::TypeError
    } else {
        NativeErrorClass::Error
    };
    let mut error = NativeNodeError::from_constructor(class, spec["message"].as_string().unwrap().clone());
    if let Some(name) = spec.get("name") {
        error = error.with_property("name", name.clone());
    }
    if let Some(cause) = spec.get("cause") {
        error = error.with_cause(materialize_reason(cause));
    }
    if let Some(properties) = spec.get("properties").and_then(JsValue::as_object) {
        for (key, value) in properties {
            error = error.with_property(key.clone(), value.clone());
        }
    }
    error.into()
}

#[tokio::test]
async fn transport_cause_trees_preserve_error_branding_plain_values_and_coercion_failures() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pi/models/openrouter.json");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", directory.path().as_os_str().to_owned())],
    );
    let fixtures = [
        r#"{"kind":"value","value":null}"#,
        r#"{"kind":"value","value":false}"#,
        r#"{"kind":"value","value":42}"#,
        r#"{"kind":"value","value":"raw\ud800"}"#,
        r#"{"kind":"value","value":{"message":"lookalike","cause":"not an Error"}}"#,
        r#"{"kind":"value","value":{"toString":null}}"#,
        r#"{"kind":"error","constructor":"Error","message":"transport\udc00","properties":{"code":"fixture","detail":{"message":"data"}},"cause":{"kind":"value","value":{"message":"plain"}}}"#,
        r#"{"kind":"error","constructor":"Error","message":"outer","cause":{"kind":"error","constructor":"TypeError","message":"inner","cause":{"kind":"value","value":"leaf\ud800"}}}"#,
        r#"{"kind":"error","constructor":"Error","message":"outer","cause":{"kind":"error","constructor":"TypeError","message":"inner","cause":{"kind":"value","value":{"toString":null}}}}"#,
    ];
    for fixture in fixtures {
        let spec = pi_js_compat::js_json::parse_lossless(fixture).unwrap();
        let expected = source.request(&pi_js_compat::js_json!({
            "operation":"ensure", "force":true, "now":1766000000000_f64, "inspectError":true, "failureSpec":spec,
        }));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let fetch = Arc::new(RejectFetch {
            reason: materialize_reason(&spec),
            calls: AtomicUsize::new(0),
        });
        let actual = ModelCatalogCache::default()
            .ensure_fresh(
                &path,
                EnsureModelCatalogOptions {
                    force: true,
                    fetch: fetch.clone(),
                    now: Arc::new(|| 1766000000000_f64),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(actual.value(), expected["output"]["error"], "fixture={fixture}");
        assert_eq!(
            fetch.calls.load(Ordering::SeqCst),
            expected["output"]["calls"].as_array().unwrap().len()
        );
        assert!(!path.exists());
    }
}

struct HeaderFailureFetch;
impl Fetch for HeaderFailureFetch {
    fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async {
            Ok(HttpResponse {
                status: 500,
                status_text: String::new(),
                headers: vec![("Retry-After".into(), " 1 ".into())],
                body: Box::new(RejectBody(None)),
            })
        })
    }
}

#[tokio::test]
async fn retry_after_reads_the_normalized_fetch_header_value() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pi/models/openrouter.json");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", directory.path().as_os_str().to_owned())],
    );
    let expected = source.request(&serde_json::json!({
        "operation":"ensure", "force":true, "now":1766000000000_f64, "inspectError":true,
        "status":500, "headers":[["Retry-After"," 1 "]], "inspectRetry":true,
        "clockValues":[1766000000000_f64], "randomValues":[0.5],
    }));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(expected["output"]["waits"], serde_json::json!([1000, 1000]));
    let actual = ModelCatalogCache::default()
        .ensure_fresh(
            &path,
            EnsureModelCatalogOptions {
                force: true,
                fetch: Arc::new(HeaderFailureFetch),
                now: Arc::new(|| 1766000000000_f64),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(actual.value().to_serde().unwrap(), expected["output"]["error"]);
}

struct RejectBody(Option<CallbackError>);
impl HttpBody for RejectBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { self.0.take().map(Err) })
    }
}
struct RejectBodyFetch(AbortReason);
impl Fetch for RejectBodyFetch {
    fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let error = CallbackError::from_rejection(self.0.clone());
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                status_text: String::new(),
                headers: Vec::new(),
                body: Box::new(RejectBody(Some(error))),
            })
        })
    }
}

#[tokio::test]
async fn response_body_rejections_remain_raw_in_the_unretried_public_helper() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
    );
    for value in [
        JsValue::Null,
        JsValue::Bool(false),
        pi_js_compat::js_json!({"message":"not an Error"}),
    ] {
        let expected = source.request(&pi_js_compat::js_json!({"operation":"fetch", "inspectError":true,
            "bodyFailureSpec":{"kind":"value","value":value}}));
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let fetch = RejectBodyFetch(AbortReason::Value(value));
        let actual = pi_coding_agent::core::model_catalog_cache::fetch_openrouter_catalog(&fetch, None)
            .await
            .unwrap_err();
        assert_eq!(actual.value(), expected["output"]["error"]);
    }
}

#[tokio::test]
async fn transport_retry_uses_typeerror_constructor_not_mutable_name() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pi/models/openrouter.json");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", directory.path().as_os_str().to_owned())],
    );
    for (class, name) in [
        (NativeErrorClass::Error, "TypeError"),
        (NativeErrorClass::TypeError, "renamed"),
    ] {
        let error = NativeNodeError::from_constructor(class, "fetch failed")
            .with_property("name", JsValue::String(name.into()))
            .with_property("code", JsValue::String("fixture-code".into()));
        let fetch = Arc::new(RejectFetch {
            reason: error.into(),
            calls: AtomicUsize::new(0),
        });
        let expected = source.request(&json!({
            "operation": "ensure", "force": true, "now": 1766000000000_f64, "inspectError": true,
            "failureSpec": {"kind": "error", "constructor": class.name(), "name": name,
                "message": "fetch failed", "properties": {"code": "fixture-code"}},
        }));
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = ModelCatalogCache::default()
            .ensure_fresh(
                &path,
                EnsureModelCatalogOptions {
                    force: true,
                    fetch: fetch.clone(),
                    now: Arc::new(|| 1766000000000_f64),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(
            fetch.calls.load(Ordering::SeqCst),
            expected["output"]["calls"].as_array().unwrap().len(),
            "constructor={class:?}, name={name}"
        );
        assert_eq!(
            actual.value().to_serde().unwrap(),
            expected["output"]["error"],
            "constructor={class:?}, name={name}"
        );
        assert!(!path.exists());
    }
}
