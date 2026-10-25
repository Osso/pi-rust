//! Surviving models.json/auth behavior, through the frozen registry rather than removed facades.
use crate::shared_oracle as oracle;
use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_registry::{ModelRegistry, ProviderConfigInput, ResolvedRequestAuth},
};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

#[tokio::test]
async fn models_json_retains_raw_url_headers_keys_and_bearer_request() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let url = JsString::from("https://example.invalid/") + JsString::from_utf16(vec![0xd800]);
    let header = JsString::from("header-") + JsString::from_utf16(vec![0xdc00]);
    let model_header = JsString::from("model-") + JsString::from_utf16(vec![0xd801]);
    let config_key = JsString::from("config-") + JsString::from_utf16(vec![0xd802]);
    let stored_key = JsString::from("stored-") + JsString::from_utf16(vec![0xdc03]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("models.json");
    let config = js_json!({"providers":{"q15-raw":{
        "baseUrl":url,"api":"openai-completions","apiKey":config_key,"authHeader":true,
        "headers":{"X-Raw":header},"models":[{"id":"raw-model","headers":{"X-Model":model_header}}]
    }}});
    std::fs::write(&path, js_json::stringify_lossless(&config)).unwrap();
    for stored in [false, true] {
        let auth = if stored {
            js_json!({"q15-raw":{"type":"api_key","key":stored_key}})
        } else {
            js_json!({})
        };
        let expected = source.request(&js_json!({"operation":"resolve_config","modelsPath":path.to_str().unwrap(),"provider":"q15-raw","model":"raw-model","auth":auth}));
        assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)), "{expected:?}");
        let registry = ModelRegistry::create(
            AuthStorage::in_memory(auth.as_object().unwrap().clone()),
            Some(path.to_str().unwrap().into()),
        )
        .unwrap();
        assert_eq!(registry.get_error(), None);
        let model = registry.find("q15-raw", "raw-model").unwrap();
        assert_eq!(model.base_url, url);
        let ResolvedRequestAuth::Ok { api_key, headers, .. } = registry.get_api_key_and_headers(&model).await.unwrap()
        else {
            panic!("configured auth must resolve")
        };
        let key = if stored { &stored_key } else { &config_key };
        assert_eq!(api_key.as_ref(), Some(key));
        let headers = headers.unwrap();
        assert_eq!(headers.get("X-Raw"), Some(&Some(header.clone())));
        assert_eq!(headers.get("X-Model"), Some(&Some(model_header.clone())));
        assert_eq!(
            headers.get("Authorization"),
            Some(&Some(JsString::from("Bearer ") + key))
        );
        let lookup = registry.get_api_key_for_provider("q15-raw").await.unwrap();
        assert_eq!(lookup.as_ref(), Some(key));
        let actual = js_json!({"error":null,"model":model,"key":lookup,"request":{"baseUrl":url,"apiKey":api_key,"headers":headers}});
        assert_eq!(Some(&actual), expected.get("output"), "stored={stored}");
    }
}

#[test]
fn opaque_model_input_rejects_unpaired_identifiers_without_replacement() {
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory([] as [(&str, JsValue); 0])).unwrap();
    let model = registry.find("anthropic", "claude-opus-4-8").unwrap();
    for (field, prefix, unit) in [("provider", "anthropic", 0xd800), ("id", "claude-opus-4-8", 0xdc00)] {
        let raw = JsString::from(prefix) + JsString::from_utf16(vec![unit]);
        let mut value = js_json::to_value(&model).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert(field.into(), JsValue::String(raw.clone()));
        assert!(
            js_json::from_value::<pi_ai::types::Model>(&value).is_err(),
            "opaque scalar approval rejects {field}"
        );
        assert_eq!(value.get(field).and_then(JsValue::as_string), Some(&raw));
    }
    assert_eq!(registry.find("anthropic", "claude-opus-4-8"), Some(model));
}

#[tokio::test]
async fn unregister_removes_registered_models_and_auth_without_touching_builtins() {
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory([] as [(&str, JsValue); 0])).unwrap();
    let builtin = registry.find("anthropic", "claude-opus-4-8").unwrap();
    let mut custom = builtin.clone();
    custom.id = "q15-registered".into();
    custom.provider = "q15-registration".into();
    registry
        .register_provider(
            "q15-registration",
            ProviderConfigInput {
                api: Some(custom.api.clone()),
                base_url: Some(custom.base_url.clone()),
                api_key: Some("registered-key".into()),
                models: Some(vec![custom.clone().into()]),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        registry.find("q15-registration", "q15-registered"),
        Some(custom.clone())
    );
    assert!(registry.has_configured_auth(&custom).unwrap());
    assert_eq!(
        registry.get_api_key_for_provider("q15-registration").await.unwrap(),
        Some("registered-key".into())
    );
    registry.unregister_provider("q15-registration").unwrap();
    assert!(registry.find("q15-registration", "q15-registered").is_none());
    assert!(!registry.has_configured_auth(&custom).unwrap());
    assert_eq!(
        registry.get_api_key_for_provider("q15-registration").await.unwrap(),
        None
    );
    assert_eq!(registry.find("anthropic", "claude-opus-4-8"), Some(builtin));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
