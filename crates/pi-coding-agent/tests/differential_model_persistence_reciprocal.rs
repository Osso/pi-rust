//! Reciprocal frozen/native reads use the same files, without resetting or copying them.
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse};
use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_catalog_cache::{EnsureModelCatalogOptions, ModelCatalogCache},
    model_registry::ModelRegistry,
};
use pi_js_compat::{abort::AbortSignal, js_json, js_json::JsValue};
use std::{path::Path, sync::Arc};

#[path = "support/catalog_fetch.rs"]
mod catalog_fetch;
#[path = "support/oracle.rs"]
mod oracle;

const NOW: f64 = 1_766_000_000_000.0;

fn output(response: &JsValue) -> &JsValue {
    assert_eq!(response.get("ok"), Some(&JsValue::Bool(true)), "{response:?}");
    response.get("output").unwrap()
}

fn read_registry_snapshot(path: &Path) -> JsValue {
    let registry = ModelRegistry::create(
        AuthStorage::in_memory([] as [(&str, JsValue); 0]),
        Some(path.to_str().unwrap().into()),
    )
    .unwrap();
    let snapshot = || {
        let models: Vec<_> = registry
            .get_all()
            .into_iter()
            .filter(|model| model.provider == "reciprocal-fixture")
            .map(|model| js_json::to_value(&model).unwrap())
            .collect();
        let models = JsValue::Array(models);
        js_json!({"error":registry.get_error(),"wire":js_json::stringify_lossless(&models),"models":models})
    };
    let before = snapshot();
    assert_eq!(before.get("error"), Some(&JsValue::Null));
    assert_eq!(before.get("models").unwrap().as_array().unwrap().len(), 2);
    registry.refresh().unwrap();
    js_json!({"before":before,"after":snapshot(),"file":std::fs::read_to_string(path).unwrap()})
}

#[test]
fn models_json_ts_and_native_authored_files_are_read_by_both_registries_without_rewriting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("models.json");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "model-persistence-oracle.ts",
        "packages/coding-agent/src/core",
        &[("PI_CODING_AGENT_DIR", dir.path().as_os_str().to_owned())],
    );
    // Unknown fields and insertion order are file data. The registry deliberately
    // projects declared fields; compare the complete projection, including its wire order.
    let mut document = js_json::parse_lossless(r#"{"unknownFirst":{"raw":"\ud800"},"providers":{"reciprocal-fixture":{"unknownProvider":[1,null],"baseUrl":"https://example.invalid/","api":"openai-completions","models":[{"unknownFirst":"\udc00","id":"two","name":"Two \ud800 😀","contextWindow":8192,"maxTokens":1024,"cost":{"output":2,"input":1,"cacheRead":0,"cacheWrite":0},"compat":{"unknownCompat":{"z":"\udc01","a":true}}},{"id":"one","name":"One \udc00","unknownLast":true}]}},"unknownLast":[false,"tail"]}"#).unwrap();
    for revision in ["initial", "updated"] {
        // TS authors -> Rust reads -> TS reads back the identical file.
        let authored = source.request(&js_json!({"path":path.to_str().unwrap(),"document":document}));
        let expected = output(&authored);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes, js_json::stringify_lossless(&document).as_bytes(), "{revision}");
        assert_eq!(&read_registry_snapshot(&path), expected, "TS-authored {revision}");
        let reread = source.request(&js_json!({"path":path.to_str().unwrap()}));
        assert_eq!(output(&reread), expected);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);

        // Native fixture authorship -> frozen reader -> native reader -> frozen reread.
        std::fs::write(&path, js_json::stringify_lossless(&document)).unwrap();
        let native = read_registry_snapshot(&path);
        let reread = source.request(&js_json!({"path":path.to_str().unwrap()}));
        assert_eq!(output(&reread), &native, "native-authored {revision}");
        assert_eq!(native, read_registry_snapshot(&path));
        let final_read = source.request(&js_json!({"path":path.to_str().unwrap()}));
        assert_eq!(output(&final_read), &native);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        document.as_object_mut().unwrap().insert(
            "unknownLast".into(),
            js_json!({"updated":pi_js_compat::js_json::JsString::from_utf16(vec![0xd800])}),
        );
    }
}

struct ForbidFetch;
impl Fetch for ForbidFetch {
    fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        panic!("a reciprocal cache read must not refetch")
    }
}

async fn load_native_cache(path: &Path) -> JsValue {
    // A new module instance prevents an earlier writer's memoized models hiding a read failure.
    let cache = ModelCatalogCache::default();
    let result = cache
        .ensure_fresh(
            path,
            EnsureModelCatalogOptions {
                force: false,
                now: Arc::new(|| NOW),
                fetch: Arc::new(ForbidFetch),
            },
        )
        .await
        .unwrap();
    assert_eq!(result.source, "cache");
    assert_eq!(result.fetched_count, 0);
    let startup = ModelCatalogCache::default().load_at_startup(path, || NOW).await;
    assert_eq!(
        js_json::to_value(&startup).unwrap(),
        js_json::to_value(&result).unwrap()
    );
    js_json::to_value(&result).unwrap()
}

fn load_ts_cache(path: &Path) -> JsValue {
    let cache_home = path.parent().unwrap().parent().unwrap().parent().unwrap();
    let mut reader = spawn_catalog(cache_home);
    let response = reader
        .request(&js_json!({"operation":"ensure","now":NOW,"force":false,"failure":"cache read unexpectedly fetched"}));
    let result = output(&response);
    assert_eq!(result.get("calls"), Some(&js_json!([])));
    let value = result.get("result").unwrap().clone();
    assert_eq!(value.get("source"), Some(&JsValue::String("cache".into())));
    let startup = reader.request(&js_json!({"operation":"startup","now":NOW}));
    assert_eq!(output(&startup).get("result"), Some(&value));
    value
}

fn spawn_catalog(cache_home: &Path) -> oracle::TsOracle {
    oracle::TsOracle::spawn_with_environment(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
        &[("XDG_CACHE_HOME", cache_home.as_os_str().to_owned())],
    )
}

async fn write_native_cache(path: &Path, payload: &JsValue) -> JsValue {
    let result = ModelCatalogCache::default()
        .ensure_fresh(
            path,
            EnsureModelCatalogOptions {
                force: true,
                now: Arc::new(|| NOW),
                fetch: Arc::new(catalog_fetch::CatalogFetch(payload.clone())),
            },
        )
        .await
        .unwrap();
    assert_eq!(result.source, "network");
    assert_eq!(result.fetched_count, 2);
    js_json::to_value(&result.models).unwrap()
}

fn write_ts_cache(cache_home: &Path, payload: &JsValue) -> JsValue {
    let mut writer = spawn_catalog(cache_home);
    let response = writer.request(&js_json!({"operation":"ensure","now":NOW,"force":true,"payload":payload}));
    let result = output(&response);
    assert_eq!(result.get("calls").unwrap().as_array().unwrap().len(), 1);
    let result = result.get("result").unwrap();
    assert_eq!(result.get("source"), Some(&JsValue::String("network".into())));
    assert_eq!(result.get("fetchedCount"), Some(&js_json!(2)));
    result.get("models").unwrap().clone()
}

fn assert_cached_models(result: &JsValue, expected: &JsValue) {
    // Equality alone does not prove property order; compare the complete serialized arrays too.
    let models = result.get("models").unwrap();
    assert_eq!(models, expected);
    assert_eq!(
        js_json::stringify_lossless(models),
        js_json::stringify_lossless(expected)
    );
}

#[tokio::test]
async fn catalog_refresh_ts_native_ts_and_native_ts_native_share_exact_cache_bytes() {
    let payload = js_json::parse_lossless(r#"{"data":[{"unknownFirst":{"raw":"\ud800"},"id":"reciprocal-two","name":"Two \ud800 😀","supported_parameters":["tools","reasoning"],"architecture":{"modality":"text+image->text"},"pricing":{"prompt":"0.00000125","completion":"0.0000025","input_cache_read":"0.0000005"},"context_length":8192.5,"top_provider":{"max_completion_tokens":512.25}},{"id":"reciprocal-one","name":"One \udc00","supported_parameters":["tools"],"unknownLast":true}]}"#).unwrap();
    for ts_first in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi/models/openrouter.json");
        let expected = if ts_first {
            write_ts_cache(dir.path(), &payload)
        } else {
            write_native_cache(&path, &payload).await
        };
        let first_bytes = std::fs::read(&path).unwrap();
        let first_reader = if ts_first {
            load_native_cache(&path).await
        } else {
            load_ts_cache(&path)
        };
        assert_cached_models(&first_reader, &expected);
        assert_eq!(std::fs::read(&path).unwrap(), first_bytes);
        // The other implementation force-refreshes the SAME file; no reset/delete/copy.
        let rewritten = if ts_first {
            write_native_cache(&path, &payload).await
        } else {
            write_ts_cache(dir.path(), &payload)
        };
        assert_eq!(
            js_json::stringify_lossless(&rewritten),
            js_json::stringify_lossless(&expected)
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            first_bytes,
            "writer bytes, ts_first={ts_first}"
        );
        let final_reader = if ts_first {
            load_ts_cache(&path)
        } else {
            load_native_cache(&path).await
        };
        assert_cached_models(&final_reader, &expected);
        assert_eq!(std::fs::read(&path).unwrap(), first_bytes);
    }
}
