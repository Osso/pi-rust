//! Source public refresh provenance, callback-return cancellation/write failure/rescue ordering.
use pi_ai::{
    auth::types::AuthOperationOptions,
    auth::{AuthError, OAuthCredential},
    oauth::*,
    request::BoxFuture,
};
use pi_coding_agent::core::auth_storage::{
    AsyncLockOperation, AuthStorage, AuthStorageBackend, FileAuthStorageBackend, GetApiKeyOptions, LockOperation,
    StorageError,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::SystemTime,
};
use tokio_util::sync::CancellationToken;
static SERIAL: Mutex<()> = Mutex::new(());
struct Fake {
    count: Arc<AtomicUsize>,
    fail: bool,
}
impl OAuthProviderInterface for Fake {
    fn id(&self) -> &str {
        "phase1-rotation"
    }
    fn name(&self) -> &str {
        "Synthetic rotation"
    }
    fn login(&self, _: Arc<dyn OAuthLoginCallbacks>) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        Box::pin(async { Ok(OAuthCredential::new("A", "old", 0.0)) })
    }
    fn refresh_token(&self, current: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredentials, AuthError>> {
        self.count.fetch_add(1, Ordering::SeqCst);
        let fail = self.fail;
        Box::pin(async move {
            if fail {
                return Err(AuthError::failed("server rejected A"));
            };
            assert!(current.refresh == "A" || current.refresh == "B");
            let mut next = OAuthCredential::new("B", "new", 4_000_000_000_000.0);
            next.extra.insert("accountId".into(), json!("synthetic"));
            Ok(next)
        })
    }
    fn get_api_key(&self, c: &OAuthCredentials) -> String {
        c.access.clone()
    }
}
struct BoundaryBackend {
    inner: FileAuthStorageBackend,
    signal: CancellationToken,
    fault: &'static str,
    syncs: AtomicUsize,
}
impl AuthStorageBackend for BoundaryBackend {
    fn with_lock(&self, operation: LockOperation<'_>) -> Result<(), StorageError> {
        let n = self.syncs.fetch_add(1, Ordering::SeqCst);
        if self.fault == "rescue-fails" && n == 1 {
            return Err(StorageError::new("rescue denied"));
        }
        self.inner.with_lock(operation)
    }
    fn with_lock_async(
        &self,
        operation: AsyncLockOperation,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<(), StorageError>> {
        let signal = self.signal.clone();
        let fault = self.fault;
        self.inner.with_lock_async(
            Box::new(move |current| {
                Box::pin(async move {
                    let next = operation(current).await?;
                    // Barrier is AFTER fake provider result and storage callback, BEFORE native write.
                    if fault == "cancel" {
                        signal.cancel();
                        return Ok(next);
                    }
                    Err(StorageError::new("locked write denied"))
                })
            }),
            options,
        )
    }
}
fn fixture() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "rotation-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    let f = p.join("auth.json");
    fs::write(&f,json!({"phase1-rotation":{"type":"oauth","refresh":"A","access":"old","expires":0},"unrelated":{"sentinel":[1,2,3]}}).to_string()).unwrap();
    f
}
fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
#[test]
fn successful_rotation_callback_then_cancel_or_write_failure_rescues_exact_result() {
    let _serial = SERIAL.lock().unwrap();
    for fault in ["cancel", "write-fails"] {
        let file = fixture();
        let signal = CancellationToken::new();
        let count = Arc::new(AtomicUsize::new(0));
        register_oauth_provider(Arc::new(Fake {
            count: count.clone(),
            fail: false,
        }));
        let storage = AuthStorage::from_storage(Arc::new(BoundaryBackend {
            inner: FileAuthStorageBackend::new(file.to_str().unwrap()),
            signal: signal.clone(),
            fault,
            syncs: AtomicUsize::new(0),
        }));
        let result = rt()
            .block_on(storage.get_api_key(
                "phase1-rotation",
                GetApiKeyOptions {
                    signal: Some(signal),
                    ..Default::default()
                },
            ))
            .unwrap();
        assert_eq!(result.as_deref(), Some("new"));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let fresh: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(fresh["phase1-rotation"]["refresh"], "B");
        assert_eq!(fresh["unrelated"], json!({"sentinel":[1,2,3]}));
        assert_eq!(storage.drain_errors().len(), 1);
        let fresh_store = AuthStorage::create(Some(file.to_str().unwrap()));
        assert_eq!(fresh_store.get("phase1-rotation").unwrap()["refresh"], "B");
    }
    unregister_oauth_provider("phase1-rotation");
}
#[test]
fn unsuccessful_refresh_does_not_rescue_and_failed_rescue_surfaces_source_error() {
    let _serial = SERIAL.lock().unwrap();
    for (fail, fault) in [(true, "write-fails"), (false, "rescue-fails")] {
        let file = fixture();
        let count = Arc::new(AtomicUsize::new(0));
        register_oauth_provider(Arc::new(Fake {
            count: count.clone(),
            fail,
        }));
        let storage = AuthStorage::from_storage(Arc::new(BoundaryBackend {
            inner: FileAuthStorageBackend::new(file.to_str().unwrap()),
            signal: CancellationToken::new(),
            fault,
            syncs: AtomicUsize::new(0),
        }));
        let error = rt()
            .block_on(storage.get_api_key(
                "phase1-rotation",
                GetApiKeyOptions {
                    throw_on_refresh_failure: true,
                    ..Default::default()
                },
            ))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains(if fail { "server rejected A" } else { "rescue denied" }),
            "{error}"
        );
        let fresh: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(fresh["phase1-rotation"]["refresh"], "A");
        assert_eq!(fresh["unrelated"], json!({"sentinel":[1,2,3]}));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    unregister_oauth_provider("phase1-rotation");
}
