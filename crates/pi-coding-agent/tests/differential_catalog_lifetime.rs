use crate::shared_catalog_fetch as catalog_fetch;
#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_oracle as oracle;
use pi_coding_agent::{
    core::{
        auth_storage::AuthStorage,
        model_catalog_cache::{get_openrouter_catalog_models, load_openrouter_catalog_at_startup},
        model_registry::ModelRegistry,
    },
    main_runner::{format_model_catalog_refresh_summary, refresh_openrouter_catalog},
};
use pi_js_compat::{
    js_format,
    js_json::{self, JsString, JsValue},
};
use std::{path::Path, sync::Arc};

fn snapshots(models: &[JsValue], ids: &[JsString]) -> JsValue {
    let found: Vec<_> = ids
        .iter()
        .map(|id| {
            models
                .iter()
                .find(|model| {
                    model
                        .get("provider")
                        .and_then(JsValue::as_string)
                        .is_some_and(|provider| provider == "openrouter")
                        && model.get("id").and_then(JsValue::as_string) == Some(id)
                })
                .cloned()
                .unwrap_or(JsValue::Null)
        })
        .collect();
    let wires: Vec<_> = found
        .iter()
        .map(|model| {
            if model == &JsValue::Null {
                JsValue::Null
            } else {
                JsValue::String(js_json::stringify_lossless(model).into())
            }
        })
        .collect();
    pi_js_compat::js_json!({"models":found,"wires":wires})
}
fn registry_snapshot(registry: &ModelRegistry, ids: &[JsString]) -> JsValue {
    snapshots(
        &registry
            .get_all()
            .iter()
            .map(|model| js_json::to_value(model).unwrap())
            .collect::<Vec<_>>(),
        ids,
    )
}
fn cache_bytes(model: JsValue) -> Vec<u8> {
    js_json::stringify_lossless(&pi_js_compat::js_json!({"fetchedAt":"2099-01-01T00:00:00.000Z","models":[model]}))
        .into_bytes()
}
fn write_cache(home: &Path, bytes: &[u8]) {
    let path = home.join("pi/models/openrouter.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn global_catalog_registry_snapshots_force_summary_and_dynamic_root_match_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let agent = root.join("agent");
    let project = root.join("project");
    let cache_a = root.join("cache-a");
    let cache_b = root.join("cache-b");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let auth = agent.join("auth.json");
    std::fs::write(&auth, r#"{"openrouter":{"type":"api_key","key":"fixture"}}"#).unwrap();
    let model_a=js_json::parse_lossless(r#"{"extraBefore":{"native":"\ud800"},"id":"zzq15-cache-a","name":"A\ud800","provider":"openrouter","api":"openai-completions","baseUrl":"https://openrouter.ai/api/v1","reasoning":false,"input":["text"],"cost":{"input":1,"costExtra":"\ud800","output":2,"cacheRead":3,"cacheWrite":4},"contextWindow":4096.75,"maxTokens":128.25,"extraAfter":[true,null]}"#).unwrap();
    let mut model_b = model_a.clone();
    model_b
        .as_object_mut()
        .unwrap()
        .insert("id".into(), JsValue::String("zzq15-cache-b".into()));
    model_b
        .as_object_mut()
        .unwrap()
        .insert("name".into(), JsValue::String(JsString::from_utf16(vec![66, 0xdc00])));
    let initial = cache_bytes(model_a);
    write_cache(&cache_a, &initial);
    write_cache(&cache_b, &cache_bytes(model_b));
    let payload=js_json::parse_lossless(r#"{"data":[{"id":"zzq15-network","name":"Network \udc00","supported_parameters":["tools","reasoning"],"architecture":{"modality":"text+image->text"},"pricing":{"prompt":"0.000001","completion":"0.000002"},"context_length":8192.5,"top_provider":{"max_completion_tokens":512.25}}]}"#).unwrap();
    let request = pi_js_compat::js_json!({"id":1,"operation":"graph","authPath":auth.to_str().unwrap(),"modelsPath":agent.join("models.json").to_str().unwrap(),"modelIds":["zzq15-cache-a","zzq15-network","zzq15-cache-b"],"payload":payload,"cacheHomeB":cache_b.to_str().unwrap(),"cwd":project.to_str().unwrap(),"args":["--refresh-models","--no-supervisor","--no-session","-ne"]});
    let pairs = vec![
        ("XDG_CACHE_HOME", cache_a.clone().into_os_string()),
        ("PI_CODING_AGENT_DIR", agent.clone().into_os_string()),
        ("PI_CODING_AGENT_STATE_DIR", root.join("state").into_os_string()),
        ("HOME", root.join("home").into_os_string()),
        ("XDG_CONFIG_HOME", root.join("config").into_os_string()),
        ("PI_OFFLINE", "0".into()),
        ("PI_SKIP_VERSION_CHECK", "1".into()),
        ("NO_COLOR", "1".into()),
    ];
    let mut source =
        oracle::TsOracle::spawn_with_environment("catalog-startup-oracle.mts", "packages/coding-agent/src", &pairs);
    let expected = source.request(&request);
    assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)), "{expected:?}");
    write_cache(&cache_a, &initial); // Reset the input after the source's real forced write; timestamps are proved separately.
    let request_path = root.join("request.json");
    std::fs::write(&request_path, js_json::stringify_lossless(&request)).unwrap();
    let mut native_pairs = pairs;
    native_pairs.push(("PI_TEST_CATALOG_REQUEST", request_path.into_os_string()));
    let output = env_probe::run_probe("catalog_registry_lifetime_probe", &native_pairs);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let wire = stdout
        .lines()
        .find_map(|line| line.strip_prefix("CATALOG_RESULT:"))
        .expect("probe result");
    let actual = js_json::parse_lossless(wire).unwrap();
    assert_eq!(&actual, expected.get("output").unwrap());
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "isolated module lifetime probe"]
async fn catalog_registry_lifetime_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let request =
        js_json::parse_lossless(std::fs::read_to_string(std::env::var_os("PI_TEST_CATALOG_REQUEST").unwrap()).unwrap())
            .unwrap();
    let ids: Vec<_> = request
        .get("modelIds")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_string().unwrap().clone())
        .collect();
    load_openrouter_catalog_at_startup().await.unwrap();
    let storage = AuthStorage::create(Some(request.get("authPath").unwrap().as_string().unwrap())).unwrap();
    let registry = ModelRegistry::create(
        storage,
        Some(request.get("modelsPath").unwrap().as_string().unwrap().clone()),
    )
    .unwrap();
    let before = registry_snapshot(&registry, &ids);
    let result = refresh_openrouter_catalog(
        false,
        true,
        Arc::new(catalog_fetch::CatalogFetch(request.get("payload").unwrap().clone())),
    )
    .await
    .unwrap();
    let refresh = pi_js_compat::js_json!({"code":0,"stdout":js_format!("{}\n",format_model_catalog_refresh_summary(&result)),"stderr":""});
    let memo_after = snapshots(&get_openrouter_catalog_models(), &ids);
    let stale = registry_snapshot(&registry, &ids);
    registry.refresh().unwrap();
    let after_refresh = registry_snapshot(&registry, &ids);
    pi_js_compat::process_env::set(
        "XDG_CACHE_HOME",
        &request
            .get("cacheHomeB")
            .unwrap()
            .as_string()
            .unwrap()
            .to_utf8()
            .unwrap(),
    );
    let root_result = load_openrouter_catalog_at_startup().await.unwrap();
    let stale_after_root = registry_snapshot(&registry, &ids);
    registry.refresh().unwrap();
    let after_root = registry_snapshot(&registry, &ids);
    let output = pi_js_compat::js_json!({"before":before,"refresh":refresh,"memoAfterRefresh":memo_after,"stale":stale,"afterRefresh":after_refresh,"rootResult":js_json::to_value(&root_result).unwrap(),"staleAfterRoot":stale_after_root,"afterRoot":after_root});
    println!("\nCATALOG_RESULT:{}", js_json::stringify_lossless(&output));
}

// Shared support is compiled once per integration target.
#[path = "support/catalog_fetch.rs"]
pub mod shared_catalog_fetch;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
