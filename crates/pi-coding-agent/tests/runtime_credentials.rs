//! Runtime overrides through the frozen public AuthStorage API.

use pi_ai::request::BoxFuture;
use pi_coding_agent::core::auth_storage::{
    AsyncLockOperation, AuthStorage, AuthStorageBackend, GetApiKeyOptions, LockOperation, StorageError,
};
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn api_key(key: &str) -> JsValue {
    pi_js_compat::js_json!({"type":"api_key", "key":key})
}

#[tokio::test]
async fn runtime_overrides_mask_stored_credentials_without_persisting() {
    let storage = AuthStorage::in_memory([("fixture", api_key("stored-key"))]);
    storage.set_runtime_api_key("fixture", &JsString::from("runtime-key"));
    assert_eq!(
        storage
            .get_api_key("fixture", GetApiKeyOptions::default())
            .await
            .unwrap(),
        Some("runtime-key".into())
    );
    assert_eq!(storage.get("fixture"), Some(api_key("stored-key")));
    storage.remove_runtime_api_key("fixture");
    assert_eq!(
        storage
            .get_api_key("fixture", GetApiKeyOptions::default())
            .await
            .unwrap(),
        Some("stored-key".into())
    );
}

struct FailingRemovalBackend {
    reads: AtomicUsize,
}
impl AuthStorageBackend for FailingRemovalBackend {
    fn with_lock(&self, operation: LockOperation<'_>) -> Result<(), StorageError> {
        if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
            operation(Some(r#"{"fixture":{"type":"api_key","key":"stored-key"}}"#))?;
            Ok(())
        } else {
            Err(StorageError::new("persistent removal failed"))
        }
    }
    fn with_lock_async(&self, _: AsyncLockOperation) -> BoxFuture<'_, Result<(), StorageError>> {
        Box::pin(async { Err(StorageError::new("unexpected async operation")) })
    }
}

#[tokio::test]
async fn keeps_runtime_override_and_stored_snapshot_after_removal_failure() {
    let storage = AuthStorage::from_storage(Arc::new(FailingRemovalBackend {
        reads: AtomicUsize::new(0),
    }));
    storage.set_runtime_api_key("fixture", &JsString::from("runtime-key"));
    assert_eq!(
        storage.remove("fixture").unwrap_err().message(),
        "persistent removal failed"
    );
    assert_eq!(storage.get("fixture"), Some(api_key("stored-key")));
    assert_eq!(
        storage
            .get_api_key("fixture", GetApiKeyOptions::default())
            .await
            .unwrap(),
        Some("runtime-key".into())
    );
}

#[tokio::test]
async fn logout_keeps_runtime_override_until_it_is_explicitly_removed() {
    let storage = AuthStorage::in_memory([("fixture", api_key("stored-key"))]);
    storage.set_runtime_api_key("fixture", &JsString::from("runtime-key"));
    storage.logout("fixture").unwrap();
    assert_eq!(storage.get("fixture"), None);
    assert_eq!(
        storage
            .get_api_key("fixture", GetApiKeyOptions::default())
            .await
            .unwrap(),
        Some("runtime-key".into())
    );
    storage.remove_runtime_api_key("fixture");
    assert_eq!(
        storage
            .get_api_key("fixture", GetApiKeyOptions::default())
            .await
            .unwrap(),
        None
    );
}
