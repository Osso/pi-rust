//! Header names use the approved opaque-scalar domain; values retain native UTF-16.
use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_catalog_cache::load_openrouter_catalog_at_startup,
    model_registry::{ModelRegistry, ResolvedRequestAuth},
};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue, stringify_lossless},
};

fn empty_auth() -> AuthStorage {
    AuthStorage::in_memory([] as [(&str, JsValue); 0])
}

#[test]
fn models_json_rejects_unpaired_header_names_without_panicking_or_replacing_them() {
    let mut failures = Vec::new();
    for unit in [0xd800, 0xdfff] {
        for location in ["provider", "model", "override"] {
            let name = JsString::from("X-") + JsString::from_utf16(vec![unit]);
            let headers = JsValue::Object([(name, JsValue::String("value".into()))].into());
            let mut config = js_json!({"providers":{"opaque-header-provider":{
                "baseUrl":"https://example.invalid/v1","api":"openai-completions",
                "models":[{"id":"opaque-header-model"}]
            }}});
            let (provider, model) = match location {
                "provider" => {
                    config["providers"]["opaque-header-provider"]
                        .as_object_mut()
                        .unwrap()
                        .insert("headers".into(), headers);
                    ("opaque-header-provider", None)
                }
                "model" => {
                    config["providers"]["opaque-header-provider"]["models"]
                        .as_array_mut()
                        .unwrap()[0]
                        .as_object_mut()
                        .unwrap()
                        .insert("headers".into(), headers);
                    ("opaque-header-provider", Some("opaque-header-model"))
                }
                _ => {
                    config = js_json!({"providers":{"anthropic":{"modelOverrides":{"claude-opus-4-8":{"headers":headers}}}}});
                    ("anthropic", Some("claude-opus-4-8"))
                }
            };
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("models.json");
            std::fs::write(&path, stringify_lossless(&config)).unwrap();
            let result = ModelRegistry::create(empty_auth(), Some(path.to_str().unwrap().into()));
            let error = match result {
                Ok(registry) => registry.get_error(),
                Err(error) => Some(error.to_string().into()),
            };
            let Some(error) = error else {
                failures.push(format!("{location}/{unit:x}: no explicit rejection"));
                continue;
            };
            let text = error.to_utf8().expect("diagnostic must not replace unpaired names");
            let names_context = text.contains(provider) && model.is_none_or(|model| text.contains(model));
            if !names_context
                || !text.contains("header name")
                || !text.contains("unpaired UTF-16")
                || text.contains('\u{fffd}')
            {
                failures.push(format!("{location}/{unit:x}: {text}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn models_json_keeps_scalar_header_names_and_exact_header_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models.json");
    let value = JsString::from("human-") + JsString::from_utf16(vec![0xd800]);
    let config = js_json!({"providers":{"opaque-header-provider":{
        "baseUrl":"https://example.invalid/v1","api":"openai-completions",
        "headers":{"X-😀":value},"models":[{"id":"opaque-header-model","headers":{"X-Model":value}}]
    }}});
    std::fs::write(&path, stringify_lossless(&config)).unwrap();
    let registry = ModelRegistry::create(empty_auth(), Some(path.to_str().unwrap().into())).unwrap();
    assert_eq!(registry.get_error(), None);
    let model = registry.find("opaque-header-provider", "opaque-header-model").unwrap();
    let ResolvedRequestAuth::Ok {
        headers: Some(headers), ..
    } = registry.get_api_key_and_headers(&model).await.unwrap()
    else {
        panic!("configured headers must resolve")
    };
    assert_eq!(headers.get("X-😀"), Some(&Some(value.clone())));
    assert_eq!(headers.get("X-Model"), Some(&Some(value)));
}

#[test]
fn cached_model_header_name_rejection_crosses_startup_and_registry_ingress() {
    for unit in [0xd800, 0xdfff] {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("pi/models/openrouter.json");
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        let mut bad = pi_ai::providers::models_data::builtin_model("openrouter", "openai/gpt-5").unwrap();
        bad.id = "bad-header-model".into();
        let mut good = bad.clone();
        good.id = "good-header-model".into();
        let value = JsString::from("human-") + JsString::from_utf16(vec![0xd800]);
        good.headers = Some([("X-😀".into(), value)].into());
        let mut bad = js_json::to_value(&bad).unwrap();
        let name = JsString::from("X-") + JsString::from_utf16(vec![unit]);
        bad.as_object_mut().unwrap().insert(
            "headers".into(),
            JsValue::Object([(name, JsValue::String("value".into()))].into()),
        );
        let catalog = js_json!({"fetchedAt":"2099-01-01T00:00:00.000Z","models":[bad,good]});
        std::fs::write(cache, stringify_lossless(&catalog)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "cached_model_header_name_probe", "--nocapture"])
            .env("OPAQUE_HEADER_CACHE_PROBE", "1")
            .env("XDG_CACHE_HOME", directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[tokio::test]
async fn cached_model_header_name_probe() {
    if std::env::var_os("OPAQUE_HEADER_CACHE_PROBE").is_none() {
        return;
    }
    let loaded = load_openrouter_catalog_at_startup().await.unwrap();
    assert_eq!(loaded.source, "cache");
    let registry = ModelRegistry::in_memory(empty_auth());
    assert!(registry.find("openrouter", "bad-header-model").is_none());
    let error = registry.get_error().expect("invalid cached model must fail explicitly");
    let text = error.to_utf8().unwrap();
    assert!(
        text.contains("openrouter") && text.contains("bad-header-model"),
        "{text}"
    );
    assert!(
        text.contains("header name") && text.contains("unpaired UTF-16"),
        "{text}"
    );
    assert!(!text.contains('\u{fffd}'), "{text}");
    let good = registry.find("openrouter", "good-header-model").unwrap();
    let expected = JsString::from("human-") + JsString::from_utf16(vec![0xd800]);
    assert_eq!(good.headers.as_ref().unwrap().get("X-😀"), Some(&expected));
}
