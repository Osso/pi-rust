//! Frozen OpenRouter cache startup, merge and mapping behavior, without live HTTP.
#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_coding_agent::core::model_catalog_cache::{EnsureModelCatalogOptions, ModelCatalogCache};
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

struct Body(Option<Vec<u8>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { self.0.take().map(Ok) })
    }
}
struct ScriptedFetch(Value);
impl Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        assert_eq!(request.url, "https://openrouter.ai/api/v1/models");
        assert!(signal.is_some());
        let bytes = serde_json::to_vec(&self.0).unwrap();
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: Box::new(Body(Some(bytes))),
            })
        })
    }
}

fn cached_model() -> Value {
    json!({"id":"q15-cache-probe", "name":"Probe", "provider":"openrouter", "api":"openai-completions", "baseUrl":"https://openrouter.ai/api/v1", "reasoning":false,"input":["text"],"cost":{"input":1,"output":2,"cacheRead":3,"cacheWrite":4},"contextWindow":4096.5,"maxTokens":512.25,"custom":{"preserved":true}})
}

#[tokio::test]
async fn startup_cache_freshness_validation_and_original_objects_match_frozen_module() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let cache = ModelCatalogCache::new(path.clone());
    let now = 1_766_000_000_000.0;
    for age in [0.0, 604_799_999.0, 604_800_000.0, -1000.0] {
        let date = pi_coding_agent::utils::iso_time::format_iso_millis((now - age) as i64);
        let model = cached_model();
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({"fetchedAt":date, "models":[model]})).unwrap(),
        )
        .unwrap();
        let expected = oracle.request(&json!({"operation":"startup", "now":now}));
        let result = cache.load_at_startup(now).await;
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            expected["output"]["result"],
            "age={age}, oracle={expected}"
        );
        assert_eq!(
            JsValue::Array(cache.get_models()).to_serde().unwrap(),
            expected["output"]["memoized"]
        );
    }
    for invalid in [
        json!({"fetchedAt":"bad","models":[cached_model()]}),
        json!({"fetchedAt":"2025-12-01","models":[{"id":"incomplete"}]}),
        json!({"fetchedAt":"2025-12-01","models":{}}),
    ] {
        std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        let expected = oracle.request(&json!({"operation":"startup", "now":now}));
        assert_eq!(
            serde_json::to_value(cache.load_at_startup(now).await).unwrap(),
            expected["output"]["result"],
            "{expected}"
        );
    }
}

#[tokio::test]
async fn network_mapping_write_and_bundled_wins_match_frozen_module() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let cache = ModelCatalogCache::new(path.clone());
    let payload = json!({"data":[{"id":"q15-remote", "supported_parameters":["tools","reasoning",null],"architecture":{"modality":"text+image->text"},"pricing":{"prompt":"0.00000125 tail","completion":0.0000025,"input_cache_read":"bad"},"context_length":4096.75,"top_provider":{"max_completion_tokens":100.5}}, {"id":"not-tool-capable","supported_parameters":["reasoning"]}, {"id":"q15-remote","name":"Second","supported_parameters":["tools"]}, {"id":"openai/gpt-4o","name":"Should not replace bundled","supported_parameters":["tools"]}]});
    let now = 1_766_000_000_000.0;
    let expected = oracle.request(&json!({"operation":"ensure","force":true,"now":now,"payload":payload}));
    let expected_file = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let result = cache
        .ensure_fresh(EnsureModelCatalogOptions {
            force: true,
            fetch: Arc::new(ScriptedFetch(payload)),
            now: Arc::new(move || now),
        })
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        expected["output"]["result"],
        "{expected}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), expected_file);
}

struct ResponseFetch {
    status: u16,
    body: Vec<u8>,
    retry_after: Option<String>,
}
impl Fetch for ResponseFetch {
    fn fetch(
        &self,
        _: HttpRequest,
        _: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let status = self.status;
        let body = self.body.clone();
        let headers = self
            .retry_after
            .iter()
            .map(|value| ("retry-after".into(), value.clone()))
            .collect();
        Box::pin(async move {
            Ok(HttpResponse {
                status,
                status_text: String::new(),
                headers,
                body: Box::new(Body(Some(body))),
            })
        })
    }
}

#[tokio::test]
async fn terminal_http_schema_and_json_errors_match_frozen_module() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let cache = ModelCatalogCache::new(path);
    for (status, body, retry_after) in [
        (400, "{}", None),
        (429, "{}", Some("100")),
        (500, "{}", Some("100")),
        (200, "{}", None),
        (200, "{\"data\":[]}", None),
        (200, "{\"data\":[{\"id\":\"no-tools\"}]}", None),
        (200, "{", None),
    ] {
        let expected = oracle.request(&json!({"operation":"ensure","force":true,"now":1_766_000_000_000.0,"body":body,"status":status,"headers":retry_after.map(|value| json!({"retry-after":value}))}));
        assert_eq!(expected["ok"], false, "{expected}");
        let error = cache
            .ensure_fresh(EnsureModelCatalogOptions {
                force: true,
                fetch: Arc::new(ResponseFetch {
                    status,
                    body: body.as_bytes().to_vec(),
                    retry_after: retry_after.map(str::to_owned),
                }),
                now: Arc::new(|| 1_766_000_000_000.0),
            })
            .await
            .unwrap_err();
        assert_eq!(
            error.message.to_utf8().unwrap(),
            expected["error"]["message"].as_str().unwrap(),
            "status={status}, body={body}"
        );
        assert_eq!(error.name, expected["error"]["name"].as_str().unwrap());
    }
}

#[tokio::test]
async fn cached_unknown_properties_and_unpaired_units_remain_native() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut model = cached_model();
    model["name"] = json!("raw-name-marker");
    let wire = serde_json::to_string(&json!({"fetchedAt":"2025-12-18T00:00:00.000Z","models":[model]}))
        .unwrap()
        .replace("raw-name-marker", "\\ud800");
    std::fs::write(&path, wire).unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let request = JsValue::from(json!({"operation":"startup","now":1_766_000_000_000.0}));
    let expected = oracle.request(&request);
    let result = ModelCatalogCache::new(path).load_at_startup(1_766_000_000_000.0).await;
    let actual = pi_js_compat::js_json::parse_lossless(serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(&actual, expected.get("output").unwrap().get("result").unwrap());
}

struct FailureFetch(CallbackError);
impl Fetch for FailureFetch {
    fn fetch(
        &self,
        _: HttpRequest,
        _: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let error = self.0.clone();
        Box::pin(async move { Err(error) })
    }
}

#[tokio::test]
async fn wrapped_native_errors_preserve_cause_objects_and_raw_human_units() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let request = pi_js_compat::js_json::parse_lossless(r#"{"operation":"ensure","force":true,"now":1766000000000,"failure":"human\ud800","failureCause":"primitive cause","inspectError":true}"#).unwrap();
    let expected = oracle.request(&request);
    let mut failure = CallbackError::new(JsString::from_utf16(vec![104, 117, 109, 97, 110, 0xd800]));
    failure.name = "TypeError".into();
    failure.cause = Some("primitive cause".into());
    let error = ModelCatalogCache::new(path)
        .ensure_fresh(EnsureModelCatalogOptions {
            force: true,
            fetch: Arc::new(FailureFetch(failure)),
            now: Arc::new(|| 1_766_000_000_000.0),
        })
        .await
        .unwrap_err();
    assert_eq!(
        error.value(),
        expected.get("output").unwrap().get("error").unwrap().clone()
    );
}
