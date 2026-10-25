//! Ports `packages/coding-agent/test/auth-storage.test.ts`.

#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use pi_ai::auth::types::OAuthCredential;
use pi_ai::request::BoxFuture;
use pi_ai::types::ProviderEnv;
use pi_coding_agent::core::auth_storage::{
    AsyncLockOperation, AuthStorage, AuthStorageBackend, FileAuthStorageBackend, GetApiKeyOptions, LockOperation,
    StorageError,
};
use pi_js_compat::js_json::JsValue;
use serde_json::{Map, Value, json};

/// Serialize tests that share process-global config-value caches or environment.
static SHARED_STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SHARED_STATE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-auth-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_auth(path: &std::path::Path, data: Value) {
    fs::write(path, serde_json::to_string(&data).unwrap()).unwrap();
}

fn api_key(key: &str) -> JsValue {
    pi_js_compat::js_json!({"type": "api_key", "key": key})
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Backend wrapper that counts lock acquisitions.
struct CountingBackend {
    inner: FileAuthStorageBackend,
    locks: Arc<AtomicUsize>,
}

impl AuthStorageBackend for CountingBackend {
    fn with_lock(&self, operation: LockOperation<'_>) -> Result<(), StorageError> {
        self.locks.fetch_add(1, Ordering::SeqCst);
        self.inner.with_lock(operation)
    }

    fn with_lock_async(&self, operation: AsyncLockOperation) -> BoxFuture<'_, Result<(), StorageError>> {
        self.locks.fetch_add(1, Ordering::SeqCst);
        self.inner.with_lock_async(operation)
    }
}

#[test]
fn reads_command_backed_api_key_credentials() {
    let _guard = serial();
    let dir = temp_dir("command-key");
    let auth_path = dir.join("auth.json");
    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "!printf 'command-key'"}}),
    );
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();
    let key = runtime()
        .block_on(storage.get_api_key("anthropic", GetApiKeyOptions::default()))
        .unwrap();
    assert_eq!(key, Some("command-key".into()));
    assert_eq!(
        storage
            .get("anthropic")
            .unwrap()
            .get("key")
            .unwrap()
            .as_string()
            .unwrap(),
        "!printf 'command-key'"
    );
}

#[test]
fn returns_oauth_credentials_unchanged() {
    let _guard = serial();
    let credential = OAuthCredential::new("refresh-token", "access-token", 1_760_000_000_000.0).to_js_value();
    let storage = AuthStorage::in_memory([("anthropic", credential.clone())]);
    let read = storage.get("anthropic");
    assert_eq!(read, Some(credential));
}

#[test]
fn credential_scoped_environment_takes_precedence_and_stays_inspectable() {
    let _guard = serial();
    let dir = temp_dir("scoped");
    let auth_path = dir.join("auth.json");
    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "$SCOPED_KEY", "env": {"SCOPED_KEY": "scoped-value", "REGION": "test-region"}}}),
    );
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();
    let key = runtime()
        .block_on(storage.get_api_key("anthropic", GetApiKeyOptions::default()))
        .unwrap();
    assert_eq!(key, Some(JsString::from("scoped-value")));
    let env: ProviderEnv = pi_js_compat::js_json::from_value(&storage.get_provider_env("anthropic").unwrap()).unwrap();
    assert_eq!(env.get("SCOPED_KEY").cloned(), Some(JsString::from("scoped-value")));
    assert_eq!(env.get("REGION").cloned(), Some(JsString::from("test-region")));
}

#[test]
fn file_stores_keep_independent_snapshots_until_explicit_reload() {
    use pi_js_compat::js_json::JsValue;
    let _guard = serial();
    let dir = temp_dir("snapshots");
    let auth_path = dir.join("auth.json");
    let before = json!({"anthropic":{"type":"api_key","key":"old"}});
    let after = json!({"anthropic":{"type":"api_key","key":"new"},"openai":{"type":"api_key","key":"openai-key"}});
    let mut oracle = oracle::TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let expected = oracle.request(&json!({"id":0,"op":"snapshot-reloads","before":before,"after":after}));
    assert_eq!(expected["ok"], true, "{expected:?}");
    write_auth(&auth_path, before);
    let reads = Arc::new(AtomicUsize::new(0));
    let backend = Arc::new(CountingBackend {
        inner: FileAuthStorageBackend::new(&auth_path.to_str().unwrap().into()).unwrap(),
        locks: reads.clone(),
    });
    let first = AuthStorage::from_storage(backend.clone());
    let second = AuthStorage::from_storage(backend);
    let sample = || json!({"first":JsValue::Object(first.get_all()).to_serde().unwrap(),"second":JsValue::Object(second.get_all()).to_serde().unwrap(),"reads":reads.load(Ordering::SeqCst)});
    let initial = sample();
    write_auth(&auth_path, after);
    let external_edit = sample();
    first.reload();
    let first_reload = sample();
    second.reload();
    assert_eq!(
        json!({"initial":initial,"externalEdit":external_edit,"firstReload":first_reload,"secondReload":sample()}),
        expected["output"]
    );
}

#[test]
fn set_persists_while_preserving_unrelated_external_edits() {
    let _guard = serial();
    let dir = temp_dir("modify");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": "old"}}));
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();
    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "old"}, "openai": {"type": "api_key", "key": "external"}}),
    );

    storage.set("anthropic", api_key("new")).unwrap();

    let stored: Value = serde_json::from_str(&fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert_eq!(
        stored,
        json!({"anthropic": {"type": "api_key", "key": "new"}, "openai": {"type": "api_key", "key": "external"}})
    );
    let content = fs::read_to_string(&auth_path).unwrap();
    assert!(
        content.ends_with('}') && !content.ends_with("}\n"),
        "no trailing newline: {content:?}"
    );
}

#[test]
fn backend_without_next_keeps_the_current_credential() {
    let _guard = serial();
    let dir = temp_dir("modify-none");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": "stored"}}));
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();

    let backend = FileAuthStorageBackend::new(&JsString::from(auth_path.to_str().unwrap())).unwrap();
    let before = fs::read(&auth_path).unwrap();
    runtime()
        .block_on(backend.with_lock_async(Box::new(|current| {
            Box::pin(async move {
                assert_eq!(
                    serde_json::from_str::<Value>(current.as_deref().unwrap()).unwrap(),
                    json!({"anthropic":{"type":"api_key","key":"stored"}})
                );
                Ok(None)
            })
        })))
        .unwrap();
    assert_eq!(fs::read(&auth_path).unwrap(), before);
    storage.reload();
    assert_eq!(storage.get("anthropic"), Some(api_key("stored")));
}

#[test]
fn serializes_concurrent_public_sets() {
    let _guard = serial();
    let dir = temp_dir("concurrent");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({}));
    let first = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();
    let second = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();

    let barrier = Arc::new(std::sync::Barrier::new(2));
    let peer = Arc::clone(&barrier);
    let left = std::thread::spawn(move || {
        peer.wait();
        first.set("anthropic", api_key("anthropic-key"))
    });
    let right = std::thread::spawn(move || {
        barrier.wait();
        second.set("openai", api_key("openai-key"))
    });
    left.join().unwrap().unwrap();
    right.join().unwrap().unwrap();

    let stored: Value = serde_json::from_str(&fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert_eq!(
        stored,
        json!({"anthropic": {"type": "api_key", "key": "anthropic-key"}, "openai": {"type": "api_key", "key": "openai-key"}})
    );
}

#[test]
fn remove_preserves_other_credentials_and_external_edits() {
    let _guard = serial();
    let dir = temp_dir("delete");
    let auth_path = dir.join("auth.json");
    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "anthropic-key"}, "openai": {"type": "api_key", "key": "openai-key"}}),
    );
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();
    write_auth(
        &auth_path,
        json!({
            "anthropic": {"type": "api_key", "key": "anthropic-key"},
            "openai": {"type": "api_key", "key": "openai-key"},
            "google": {"type": "api_key", "key": "external-key"}
        }),
    );

    storage.remove("anthropic").unwrap();
    assert_eq!(storage.get("anthropic"), None);
    assert_eq!(storage.get("openai"), Some(api_key("openai-key")));
    assert_eq!(storage.get("google"), Some(api_key("external-key")));
}

#[test]
fn in_memory_storage_implements_the_same_behavior() {
    let _guard = serial();
    let mut data = Map::new();
    data.insert("anthropic".to_owned(), json!({"type": "api_key", "key": "initial"}));
    let storage = AuthStorage::in_memory(data);

    assert_eq!(storage.get("anthropic"), Some(api_key("initial")));
    storage.set("anthropic", api_key("updated")).unwrap();
    assert_eq!(storage.get("anthropic"), Some(api_key("updated")));
    storage.remove("anthropic").unwrap();
    assert_eq!(storage.get("anthropic"), None);
}

#[test]
fn does_not_write_after_a_lock_failure_and_recovers_on_retry() {
    let _guard = serial();
    let dir = temp_dir("lock-failure");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": "stored"}}));

    struct FailingOnce {
        inner: FileAuthStorageBackend,
        failed: AtomicUsize,
    }
    impl AuthStorageBackend for FailingOnce {
        fn with_lock(&self, operation: LockOperation<'_>) -> Result<(), StorageError> {
            if self.failed.fetch_add(1, Ordering::SeqCst) == 1 {
                return Err(StorageError::new("lock unavailable"));
            }
            self.inner.with_lock(operation)
        }
        fn with_lock_async(&self, operation: AsyncLockOperation) -> BoxFuture<'_, Result<(), StorageError>> {
            self.inner.with_lock_async(operation)
        }
    }

    let backend = Arc::new(FailingOnce {
        inner: FileAuthStorageBackend::new(&JsString::from(&auth_path.display().to_string())).unwrap(),
        failed: AtomicUsize::new(0),
    });
    let storage = AuthStorage::from_storage(backend);

    let error = storage.set("openai", api_key("new")).unwrap_err();
    assert!(error.to_string().contains("lock unavailable"), "error: {error:?}");
    let stored: Value = serde_json::from_str(&fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert_eq!(stored, json!({"anthropic": {"type": "api_key", "key": "stored"}}));

    storage.set("openai", api_key("new")).unwrap();
    let stored: Value = serde_json::from_str(&fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert_eq!(
        stored,
        json!({"anthropic": {"type": "api_key", "key": "stored"}, "openai": {"type": "api_key", "key": "new"}})
    );
}

#[test]
fn frozen_get_reads_raw_credentials_without_resolution_execution_or_validation() {
    use pi_js_compat::js_json::JsValue;
    let _guard = serial();
    let dir = temp_dir("raw-get");
    let auth_path = dir.join("auth.json");
    let sentinel = dir.join("must-not-execute");
    let command = format!("!printf executed > {}", sentinel.display());
    let fixtures = [
        json!({"anthropic":{"type":"api_key","key":"$NOT_RESOLVED"}}),
        json!({"anthropic":{"type":"api_key","key":command}}),
        json!({"anthropic":{"type":"api_key","key":3}}),
        json!([]),
    ];
    let mut oracle = oracle::TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    for (index, fixture) in fixtures.into_iter().enumerate() {
        write_auth(&auth_path, fixture);
        let before = fs::read(&auth_path).unwrap();
        let expected = oracle
            .request(&json!({"id":index,"op":"raw-get","authPath":auth_path.to_str().unwrap(),"provider":"anthropic"}));
        assert_eq!(expected["ok"], true, "{expected:?}");
        let storage = AuthStorage::create(Some(&auth_path.to_str().unwrap().into())).unwrap();
        let credential = storage.get("anthropic").unwrap_or(JsValue::Null).to_serde().unwrap();
        let all = JsValue::Object(storage.get_all()).to_serde().unwrap();
        let providers = storage
            .list_providers()
            .into_iter()
            .map(|id| id.to_utf8().unwrap())
            .collect::<Vec<_>>();
        let errors = storage
            .drain_errors()
            .into_iter()
            .map(|error| error.message().to_utf8().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            json!({"credential":credential,"all":all,"providers":providers,"errors":errors}),
            expected["output"],
            "fixture {index}"
        );
        assert_eq!(
            fs::read(&auth_path).unwrap(),
            before,
            "raw getter must not mutate auth.json"
        );
        assert!(!sentinel.exists(), "raw getter must not execute the stored command");
    }
}

#[test]
fn stored_credentials_can_be_read_without_resolving() {
    let _guard = serial();
    let dir = temp_dir("stored");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": "$RAW"}}));
    let storage = AuthStorage::create(Some(&JsString::from(auth_path.to_str().unwrap()))).unwrap();
    assert_eq!(storage.get("anthropic"), Some(api_key("$RAW")));
    assert_eq!(storage.get("missing"), None);
    let new_path = dir.join("missing/auth.json");
    let empty = AuthStorage::create(Some(&JsString::from(new_path.to_str().unwrap()))).unwrap();
    assert_eq!(empty.get("anthropic"), None);
    assert_eq!(fs::read_to_string(new_path).unwrap(), "{}");
}

#[test]
fn auth_files_are_private() {
    let _guard = serial();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("modes");
        let auth_path = dir.join("nested/auth.json");
        let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string()))).unwrap();
        storage.remove("missing").unwrap();
        let file_mode = fs::metadata(&auth_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600);
        let dir_mode = fs::metadata(auth_path.parent().unwrap()).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
    }
}

#[test]
fn backend_callback_rejection_retains_the_frozen_value() {
    use pi_js_compat::js_json::{JsString, JsValue};
    let mut source = oracle::TsOracle::spawn(
        "auth-storage-oracle.ts",
        "packages/coding-agent/src/core/auth-storage.ts",
    );
    let reasons = [
        JsValue::Null,
        JsValue::Bool(false),
        JsValue::Number(0.0),
        JsValue::String("".into()),
        JsValue::String(JsString::from_utf16(vec![0xd800])),
        pi_js_compat::js_json!({"nested":[false,3]}),
    ];
    for reason in reasons {
        let expected = source.request(&pi_js_compat::js_json!({"id":1,"op":"backend-rejection","reason":reason}));
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let backend = pi_coding_agent::core::auth_storage::InMemoryAuthStorageBackend::new();
        backend.set_value(Some("{}".into()));
        let supplied = reason.clone();
        let error = runtime()
            .block_on(backend.with_lock_async(Box::new(move |_| {
                Box::pin(async move {
                    Err(StorageError::Failure {
                        error: Box::new(pi_ai::request::CallbackError::from_rejection(
                            pi_js_compat::abort::AbortReason::Value(supplied),
                        )),
                        cause: None,
                    })
                })
            })))
            .unwrap_err();
        let storage = AuthStorage::from_storage(Arc::new(backend));
        assert_eq!(storage.get("p1"), None);
        let actual =
            pi_js_compat::js_json!({"thrown":error.thrown_value().value(),"after":JsValue::Object(storage.get_all())});
        assert_eq!(actual, expected["output"]);
    }
}
