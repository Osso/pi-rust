use pi_ai::{
    auth::{AuthError, OAuthCredential},
    oauth::*,
    request::BoxFuture,
};
use pi_coding_agent::core::auth_storage::{AuthStorage, GetApiKeyOptions, InMemoryAuthStorageBackend};
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde_json::json;
use std::sync::Arc;
#[path = "support/oracle.rs"]
mod oracle;

const RAW: &str = r#"{"fake":{"type":"oauth","access":"a\ud800","refresh":"r\udfff","expires":0,"\ud800":{"nested":"\udfff"}},"\ud800":{},"�":{}}"#;
#[test]
fn public_getter_preserves_surrogate_values_and_distinct_keys() {
    let backend = Arc::new(InMemoryAuthStorageBackend::new());
    backend.set_value(Some(RAW.into()));
    let storage = AuthStorage::from_storage(backend);
    let value = storage.get("fake").unwrap();
    assert_eq!(
        js_json::stringify_lossless(&value),
        r#"{"type":"oauth","access":"a\ud800","refresh":"r\udfff","expires":0,"\ud800":{"nested":"\udfff"}}"#
    );
    assert_eq!(
        storage.list_providers(),
        ["fake".into(), JsString::from_utf16(vec![0xd800]), "�".into()]
    );
    let credential = OAuthCredential::from_js_value(&value).unwrap();
    assert_eq!(credential.refresh.units(), [114, 0xdfff]);
    assert_eq!(credential.access.units(), [97, 0xd800]);
    assert_eq!(credential.to_js_value(), value);
    storage.set("fake", credential.to_js_value()).unwrap();
    assert_eq!(storage.get("fake").unwrap(), value);
}

struct LosslessProvider;
impl OAuthProviderInterface for LosslessProvider {
    fn id(&self) -> &str {
        "fake"
    }
    fn name(&self) -> &str {
        "Synthetic UTF16"
    }
    fn login(&self, _: Arc<dyn OAuthLoginCallbacks>) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn refresh_token(
        &self,
        mut credential: OAuthCredentials,
    ) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        Box::pin(async move {
            assert_eq!(credential.refresh.units(), [114, 0xdfff]);
            assert_eq!(credential.access.units(), [97, 0xd800]);
            assert_eq!(
                credential
                    .extra
                    .get(&JsString::from_utf16(vec![0xd800]))
                    .unwrap()
                    .get("nested")
                    .unwrap()
                    .as_string()
                    .unwrap()
                    .units(),
                [0xdfff]
            );
            credential.refresh = JsString::from_utf16(vec![66, 0xdfff]);
            credential.access = "new".into();
            credential.expires = 4_000_000_000_000.0;
            Ok(credential)
        })
    }
    fn get_api_key(&self, credential: &OAuthCredentials) -> Result<JsString, pi_ai::request::CallbackError> {
        Ok(credential.access.clone())
    }
}
#[tokio::test]
async fn touched_refresh_and_public_roundtrip_match_ts_without_surrogate_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("auth.json");
    std::fs::write(&path, RAW).unwrap();
    let source_path = directory.path().join("source-auth.json");
    std::fs::write(&source_path, RAW).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let frozen_rotation = source.request(&json!({"id":0,"op":"rotate-in-place","authPath":source_path}));
    assert_eq!(frozen_rotation["ok"], true, "{frozen_rotation}");
    let storage = AuthStorage::create(Some(&JsString::from(path.to_str().unwrap()))).unwrap();
    register_oauth_provider(Arc::new(LosslessProvider));
    let key = storage.get_api_key("fake", GetApiKeyOptions::default()).await.unwrap();
    assert_eq!(key, Some(JsString::from("new")));
    let value = storage.get("fake").unwrap();
    let expected = frozen_rotation["output"]["credential"].as_str().unwrap();
    assert_eq!(js_json::stringify_lossless(&value), expected);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        frozen_rotation["output"]["file"]
    );
    let snapshot = source.request(&json!({"id":1,"op":"lossless","authPath":path,"provider":"fake"}));
    assert_eq!(snapshot["ok"], true, "{snapshot}");
    assert_eq!(snapshot["output"]["credential"], expected);
    assert_eq!(
        snapshot["output"]["all"],
        js_json::stringify_lossless(&JsValue::Object(storage.get_all()))
    );
    let entries = JsValue::Array(storage.list_providers().into_iter().map(JsValue::String).collect());
    assert_eq!(snapshot["output"]["entries"], js_json::stringify_lossless(&entries));
    let lone_key = JsString::from_utf16(vec![0xd800]);
    let updated =
        r#"{"type":"oauth","refresh":"\udfff","access":"\ud800","expires":4000000000000,"env":{"\ud800":"\udfff"}}"#;
    let snapshot = source.request(
        &json!({"id":2,"op":"lossless","authPath":path,"providerJson":r#""\ud800""#,"credentialJson":updated}),
    );
    assert_eq!(snapshot["ok"], true, "{snapshot}");
    storage.reload();
    assert_eq!(
        js_json::stringify_lossless(&storage.get(lone_key.clone()).unwrap()),
        updated
    );
    storage
        .set(lone_key.clone(), storage.get(lone_key.clone()).unwrap())
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        snapshot["output"]["file"].as_str().unwrap()
    );
    storage.remove(lone_key).unwrap();
    assert!(storage.has("�"));
    storage.set("fake", js_json::parse_lossless(updated).unwrap()).unwrap();
    let mut boundary = oracle::TsOracle::spawn(
        "auth-refresh-boundaries-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let frozen = boundary.request(&json!({"id":1,"op":"surrogate"}));
    assert_eq!(frozen["ok"], true, "{frozen}");
    assert_eq!(frozen["output"]["units"], json!([55296]));
    let key = storage
        .get_api_key("fake", GetApiKeyOptions::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(json!(key.units()), frozen["output"]["units"]);
    assert_eq!(
        js_json::stringify_lossless(&JsValue::String(key)),
        frozen["output"]["resultJson"]
    );
    assert_eq!(js_json::stringify_lossless(&storage.get("fake").unwrap()), updated);
    unregister_oauth_provider("fake");
}

struct OverlappingProvider {
    reads: Arc<std::sync::Mutex<Vec<String>>>,
    overlap: Arc<tokio::sync::Barrier>,
}
impl OAuthProviderInterface for OverlappingProvider {
    fn id(&self) -> &str {
        "auth-refresh-boundary"
    }
    fn name(&self) -> &str {
        "Synthetic overlap"
    }
    fn login(&self, _: Arc<dyn OAuthLoginCallbacks>) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        Box::pin(async { unreachable!() })
    }
    fn refresh_token(&self, credential: OAuthCredentials) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        let reads = self.reads.clone();
        let overlap = self.overlap.clone();
        Box::pin(async move {
            reads.lock().unwrap().push(credential.refresh.to_utf8().unwrap());
            overlap.wait().await;
            Ok(OAuthCredential::new("B", "new", 4_000_000_000_000.0))
        })
    }
    fn get_api_key(&self, credential: &OAuthCredentials) -> Result<JsString, pi_ai::request::CallbackError> {
        Ok(credential.access.clone())
    }
}

#[tokio::test]
async fn overlapping_expired_in_memory_refreshes_both_read_a_like_frozen() {
    let mut source = oracle::TsOracle::spawn(
        "auth-refresh-boundaries-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let frozen = source.request(&json!({"id":1,"op":"overlap"}));
    assert_eq!(frozen["ok"], true, "{frozen}");
    assert_eq!(frozen["output"]["reads"], json!(["A", "A"]));
    let reads = Arc::new(std::sync::Mutex::new(Vec::new()));
    register_oauth_provider(Arc::new(OverlappingProvider {
        reads: reads.clone(),
        overlap: Arc::new(tokio::sync::Barrier::new(2)),
    }));
    let id = "auth-refresh-boundary";
    let storage = AuthStorage::in_memory([(id, OAuthCredential::new("A", "old", 0.0).to_js_value())]);
    let results = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(
            storage.get_api_key(id, GetApiKeyOptions::default()),
            storage.get_api_key(id, GetApiKeyOptions::default())
        )
    })
    .await;
    unregister_oauth_provider(id);
    let (first, second) = results.expect("both refreshes must enter provider before either finishes");
    let native = json!({
        "reads": *reads.lock().unwrap(),
        "results": [first.unwrap().unwrap().to_utf8().unwrap(), second.unwrap().unwrap().to_utf8().unwrap()],
        "credential": storage.get(id).unwrap().to_serde().unwrap(),
    });
    assert_eq!(native, frozen["output"]);
}

#[test]
fn public_missing_get_does_not_add_credentials_to_the_created_auth_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("not-created/auth.json");
    let storage = AuthStorage::create(Some(&JsString::from(path.to_str().unwrap()))).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(storage.get("fake"), None);
    assert!(storage.list_providers().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(before, b"{}");
}
