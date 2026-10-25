//! Frozen OpenRouter cache startup, merge and mapping behavior, without live HTTP.
use crate::shared_oracle as oracle;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_coding_agent::core::model_catalog_cache::{EnsureModelCatalogOptions, ModelCatalogCache};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};
use std::sync::Arc;

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
        signal: Option<AbortSignal>,
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
    let cache = ModelCatalogCache::default();
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
        let result = cache.load_at_startup(&path, || now).await;
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
            serde_json::to_value(cache.load_at_startup(&path, || now).await).unwrap(),
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
    let cache = ModelCatalogCache::default();
    let payload = json!({"data":[{"id":"q15-remote", "supported_parameters":["tools","reasoning",null],"architecture":{"modality":"text+image->text"},"pricing":{"prompt":"0.00000125 tail","completion":0.0000025,"input_cache_read":"bad"},"context_length":4096.75,"top_provider":{"max_completion_tokens":100.5}}, {"id":"not-tool-capable","supported_parameters":["reasoning"]}, {"id":"q15-remote","name":"Second","supported_parameters":["tools"]}, {"id":"openai/gpt-4o","name":"Should not replace bundled","supported_parameters":["tools"]}]});
    let now = 1_766_000_000_000.0;
    let expected = oracle.request(&json!({"operation":"ensure","force":true,"now":now,"payload":payload}));
    let expected_file = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let result = cache
        .ensure_fresh(
            &path,
            EnsureModelCatalogOptions {
                force: true,
                fetch: Arc::new(ScriptedFetch(payload)),
                now: Arc::new(move || now),
            },
        )
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
    fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
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
    let cache = ModelCatalogCache::default();
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
            .ensure_fresh(
                &path,
                EnsureModelCatalogOptions {
                    force: true,
                    fetch: Arc::new(ResponseFetch {
                        status,
                        body: body.as_bytes().to_vec(),
                        retry_after: retry_after.map(str::to_owned),
                    }),
                    now: Arc::new(|| 1_766_000_000_000.0),
                },
            )
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
    let result = ModelCatalogCache::default()
        .load_at_startup(&path, || 1_766_000_000_000.0)
        .await;
    let actual = pi_js_compat::js_json::parse_lossless(serde_json::to_string(&result).unwrap()).unwrap();
    assert_eq!(&actual, expected.get("output").unwrap().get("result").unwrap());
}

struct FailureFetch(CallbackError);
impl Fetch for FailureFetch {
    fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
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
    let failure = pi_js_compat::node_error::NativeNodeError::from_constructor(
        pi_js_compat::node_error::NativeErrorClass::TypeError,
        JsString::from_utf16(vec![104, 117, 109, 97, 110, 0xd800]),
    )
    .with_property(
        "cause",
        pi_js_compat::js_json::JsValue::String("primitive cause".into()),
    )
    .into();
    let error = ModelCatalogCache::default()
        .ensure_fresh(
            &path,
            EnsureModelCatalogOptions {
                force: true,
                fetch: Arc::new(FailureFetch(failure)),
                now: Arc::new(|| 1_766_000_000_000.0),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.value(),
        expected.get("output").unwrap().get("error").unwrap().clone()
    );
}

struct SequenceFetch {
    statuses: Vec<u16>,
    calls: std::sync::atomic::AtomicUsize,
    body: Vec<u8>,
}
impl Fetch for SequenceFetch {
    fn fetch(&self, _: HttpRequest, signal: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        assert!(signal.is_some());
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let status = self.statuses[call.min(self.statuses.len() - 1)];
        let body = self.body.clone();
        Box::pin(async move {
            Ok(HttpResponse {
                status,
                status_text: String::new(),
                headers: vec![],
                body: Box::new(Body(Some(body))),
            })
        })
    }
}

#[tokio::test]
async fn retryable_http_requests_stop_on_success_or_after_three_attempts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let payload = json!({"data":[{"id":"q15-retry","supported_parameters":["tools"]}]});
    for statuses in [vec![503, 500, 200], vec![429, 200], vec![503, 503, 503], vec![400]] {
        let expected = oracle.request(&json!({"operation":"ensure","force":true,"now":1766000000000.0,"payload":payload,"statuses":statuses,"inspectError":true}));
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        let fetch = Arc::new(SequenceFetch {
            statuses,
            calls: std::sync::atomic::AtomicUsize::new(0),
            body: serde_json::to_vec(&payload).unwrap(),
        });
        let actual = ModelCatalogCache::default()
            .ensure_fresh(
                &path,
                EnsureModelCatalogOptions {
                    force: true,
                    fetch: fetch.clone(),
                    now: Arc::new(|| 1766000000000.0),
                },
            )
            .await;
        let output = &expected["output"];
        assert_eq!(
            fetch.calls.load(std::sync::atomic::Ordering::SeqCst),
            output["calls"].as_array().unwrap().len()
        );
        match actual {
            Ok(result) => assert_eq!(serde_json::to_value(result).unwrap(), output["result"]),
            Err(error) => assert_eq!(error.value().to_serde().unwrap(), output["error"]),
        }
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
    }
}

// Only the provenance-known filename generated by writeCachedCatalog is unstable.
// No arbitrary error text, argument or path is rewritten.
fn normalize_generated_cache_temporary(mut value: Value) -> Value {
    let temporary =
        regex::Regex::new(r"\.openrouter-[0-9]+-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.tmp")
            .unwrap();
    let mut normalize = |pointer: &str| {
        if let Some(field) = value.pointer_mut(pointer)
            && let Some(text) = field.as_str()
        {
            *field = Value::String(temporary.replace_all(text, ".openrouter-<pid>-<uuid>.tmp").into_owned());
        }
    };
    normalize("/message");
    normalize("/cause/message");
    normalize("/cause/path");
    value
}

#[tokio::test]
async fn failed_atomic_rename_preserves_node_metadata_and_removes_temporary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    std::fs::create_dir_all(&path).unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let payload = json!({"data":[{"id":"q15-write-fail","supported_parameters":["tools"]}]});
    let expected = oracle.request(
        &json!({"operation":"ensure","force":true,"now":1766000000000.0,"payload":payload,"inspectError":true}),
    );
    assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    let cache = ModelCatalogCache::default();
    let before = cache.get_models();
    let error = cache
        .ensure_fresh(
            &path,
            EnsureModelCatalogOptions {
                force: true,
                fetch: Arc::new(ScriptedFetch(payload)),
                now: Arc::new(|| 1766000000000.0),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        normalize_generated_cache_temporary(error.value().to_serde().unwrap()),
        normalize_generated_cache_temporary(expected["output"]["error"].clone())
    );
    assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    assert_eq!(cache.get_models(), before);
}

#[tokio::test]
async fn failed_mkdir_and_cleanup_keep_the_primary_error_and_cleanup_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    std::fs::create_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    std::fs::write(path.parent().unwrap(), "blocked by a file").unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let payload = json!({"data":[{"id":"q15-mkdir-fail","supported_parameters":["tools"]}]});
    let expected = oracle.request(
        &json!({"operation":"ensure","force":true,"now":1766000000000.0,"payload":payload,"inspectError":true}),
    );
    let error = ModelCatalogCache::default()
        .ensure_fresh(
            &path,
            EnsureModelCatalogOptions {
                force: true,
                fetch: Arc::new(ScriptedFetch(payload)),
                now: Arc::new(|| 1766000000000.0),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        normalize_generated_cache_temporary(error.value().to_serde().unwrap()),
        normalize_generated_cache_temporary(expected["output"]["error"].clone())
    );
    assert_eq!(
        std::fs::read_to_string(path.parent().unwrap()).unwrap(),
        "blocked by a file"
    );
}

#[tokio::test]
async fn direct_fetch_keeps_nonfinite_cost_and_signed_zero_until_json_serialization() {
    let dir = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let payload = json!({"data":[{"id":"q15-overflow","supported_parameters":["tools"],"pricing":{"prompt":"1e308"}},{"id":"q15-negative-zero","supported_parameters":["tools"],"pricing":{"prompt":"-0.00000000000001"}}]});
    let expected = oracle.request(&json!({"operation":"fetch","payload":payload,"inspectNumbers":true}));
    let fetch = ResponseFetch {
        status: 200,
        body: serde_json::to_vec(&payload).unwrap(),
        retry_after: None,
    };
    let models = pi_coding_agent::core::model_catalog_cache::fetch_openrouter_catalog(&fetch, None)
        .await
        .unwrap();
    let observed: Vec<_> = models.iter().map(|model| {
        let cost = model.get("cost").unwrap().get("input").unwrap();
        json!({"type":if cost.as_f64().is_some() {"number"} else {"object"},"finite":cost.as_f64().is_some_and(f64::is_finite),"negativeZero":cost.as_f64().is_some_and(|number| number == 0.0 && number.is_sign_negative())})
    }).collect();
    assert_eq!(json!(observed), expected["output"]["numericStates"]);
    let wire = pi_js_compat::js_json::stringify_lossless(&JsValue::Array(models));
    let serialized: Value = serde_json::from_str(&wire).unwrap();
    assert_eq!(serialized, expected["output"]["result"]);
    assert_eq!(
        expected["output"]["calls"],
        json!([{"url":"https://openrouter.ai/api/v1/models","signal":false}])
    );
}

#[tokio::test]
async fn typed_cached_model_keeps_interleaved_unknown_keys_in_frozen_json_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi/models/openrouter.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let model = r#"{"extraBefore":{"native":"\ud800"},"maxTokens":2000.5,"id":"q15-ordered-model","middleExtra":3,"name":"Display","cost":{"before":true,"cacheWrite":4,"tiers":[{"extraBefore":"\ud800","inputTokensAbove":100,"output":4,"cacheWrite":0,"input":2,"cacheRead":1,"extraAfter":null}],"input":1,"middle":{"native":"\ud800"},"cacheRead":3,"output":2,"after":"tail"},"provider":"openrouter","input":["text"],"api":"openai-completions","reasoning":true,"baseUrl":"https://openrouter.ai/api/v1","contextWindow":8000,"extraAfter":[true,null]}"#;
    std::fs::write(
        &path,
        format!(r#"{{"fetchedAt":"2025-12-18T00:00:00.000Z","models":[{model}]}}"#),
    )
    .unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", dir.path().as_os_str().to_owned())],
    );
    let expected = oracle.request(&JsValue::from(
        json!({"operation":"startup","now":1766000000000.0,"inspectId":"q15-ordered-model"}),
    ));
    let native: pi_ai::types::Model = serde_json::from_str(model).unwrap();
    let expected_wire = expected
        .get("output")
        .unwrap()
        .get("modelWire")
        .unwrap()
        .as_string()
        .unwrap()
        .to_utf8()
        .unwrap();
    assert_eq!(serde_json::to_string(&native).unwrap(), expected_wire);
}

#[tokio::test]
async fn typed_cached_model_keeps_native_human_name_and_rejects_opaque_unpaired_id() {
    let mut model = cached_model();
    model["name"] = json!("raw-name-marker");
    let wire = serde_json::to_string(&model)
        .unwrap()
        .replace("raw-name-marker", "\\ud800");
    let native: pi_ai::types::Model = serde_json::from_str(&wire).expect("model name is native human text");
    let serialized = pi_js_compat::js_json::parse_lossless(serde_json::to_string(&native).unwrap()).unwrap();
    assert_eq!(serialized.get("name").unwrap().as_string().unwrap().units(), &[0xd800]);
    let opaque = serde_json::to_string(&cached_model())
        .unwrap()
        .replace("q15-cache-probe", "\\ud800");
    let error = serde_json::from_str::<pi_ai::types::Model>(&opaque).unwrap_err();
    assert!(error.to_string().contains("surrogate"), "{error}");
}

#[test]
fn typed_cached_optional_null_rejects_known_fields_and_preserves_unknown_fields() {
    for field in ["headers", "compat", "thinkingLevelMap", "autoCompactionThreshold"] {
        let mut model = cached_model();
        model[field] = Value::Null;
        assert!(
            serde_json::from_value::<pi_ai::types::Model>(model).is_err(),
            "null {field} must fail explicitly"
        );
    }
    // The frozen Model no longer has samplingParams; unknown root properties remain lossless.
    let mut model = cached_model();
    model["samplingParams"] = Value::Null;
    let native: pi_ai::types::Model = serde_json::from_value(model).expect("unknown null remains representable");
    let serialized = serde_json::to_value(native).unwrap();
    assert_eq!(serialized.get("samplingParams"), Some(&Value::Null));
}
