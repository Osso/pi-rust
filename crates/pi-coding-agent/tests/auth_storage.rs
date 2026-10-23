//! Ports `packages/coding-agent/test/auth-storage.test.ts`.

#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use pi_ai::auth::types::{
    ApiKeyCredential, AuthError, AuthOperationOptions, Credential, CredentialStore, OAuthCredential,
};
use pi_ai::request::BoxFuture;
use pi_ai::types::ProviderEnv;
use pi_coding_agent::core::auth_storage::{
    AsyncLockOperation, AuthStorage, AuthStorageBackend, FileAuthStorageBackend, InMemoryAuthStorageBackend,
    LockOperation, StorageError, read_stored_credential,
};
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

fn api_key(key: &str) -> Credential {
    Credential::ApiKey(ApiKeyCredential {
        key: Some(key.to_owned().into()),
        env: None,
    })
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

    fn with_lock_async(
        &self,
        operation: AsyncLockOperation,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<(), StorageError>> {
        self.locks.fetch_add(1, Ordering::SeqCst);
        self.inner.with_lock_async(operation, options)
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
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string())));
    let credential = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap();
    assert_eq!(credential, Some(api_key("command-key")));
}

#[test]
fn returns_oauth_credentials_unchanged() {
    let _guard = serial();
    let credential = Credential::OAuth(OAuthCredential::new(
        "refresh-token",
        "access-token",
        1_760_000_000_000.0,
    ));
    let mut data = Map::new();
    data.insert("anthropic".to_owned(), serde_json::to_value(&credential).unwrap());
    let storage = AuthStorage::in_memory(data);
    let read = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap();
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
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string())));
    let credential = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap();
    let Credential::ApiKey(api_key) = credential.expect("credential") else {
        panic!("expected api key");
    };
    assert_eq!(api_key.key.as_ref(), Some(&JsString::from("scoped-value")));
    let env: ProviderEnv = api_key.env.expect("env is preserved");
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
        inner: FileAuthStorageBackend::new(&auth_path.to_str().unwrap().into()),
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
fn modify_persists_while_preserving_unrelated_external_edits() {
    let _guard = serial();
    let dir = temp_dir("modify");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": "old"}}));
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string())));
    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "old"}, "openai": {"type": "api_key", "key": "external"}}),
    );

    runtime()
        .block_on(storage.modify(
            "anthropic",
            Arc::new(|_current| Box::pin(async { Ok(Some(api_key("new"))) })),
            AuthOperationOptions::default(),
        ))
        .unwrap();

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
fn modify_without_a_replacement_keeps_the_current_credential() {
    let _guard = serial();
    let dir = temp_dir("modify-none");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": "stored"}}));
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string())));

    let result = runtime()
        .block_on(storage.modify(
            "anthropic",
            Arc::new(|_current| Box::pin(async { Ok(None) })),
            AuthOperationOptions::default(),
        ))
        .unwrap();
    assert_eq!(result, Some(api_key("stored")));
    let read = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap();
    assert_eq!(read, Some(api_key("stored")));
}

#[test]
fn serializes_concurrent_modifications() {
    let _guard = serial();
    let dir = temp_dir("concurrent");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({}));
    let first = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string())));
    let second = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string())));

    runtime().block_on(async {
        let (left, right) = tokio::join!(
            first.modify(
                "anthropic",
                Arc::new(|_current| Box::pin(async { Ok(Some(api_key("anthropic-key"))) })),
                AuthOperationOptions::default(),
            ),
            second.modify(
                "openai",
                Arc::new(|_current| Box::pin(async { Ok(Some(api_key("openai-key"))) })),
                AuthOperationOptions::default(),
            ),
        );
        left.unwrap();
        right.unwrap();
    });

    let stored: Value = serde_json::from_str(&fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert_eq!(
        stored,
        json!({"anthropic": {"type": "api_key", "key": "anthropic-key"}, "openai": {"type": "api_key", "key": "openai-key"}})
    );
}

#[test]
fn delete_removes_one_credential_while_preserving_others() {
    let _guard = serial();
    let dir = temp_dir("delete");
    let auth_path = dir.join("auth.json");
    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "anthropic-key"}, "openai": {"type": "api_key", "key": "openai-key"}}),
    );
    let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string())));
    write_auth(
        &auth_path,
        json!({
            "anthropic": {"type": "api_key", "key": "anthropic-key"},
            "openai": {"type": "api_key", "key": "openai-key"},
            "google": {"type": "api_key", "key": "external-key"}
        }),
    );

    runtime()
        .block_on(storage.delete("anthropic", AuthOperationOptions::default()))
        .unwrap();

    runtime().block_on(async {
        let listed = storage.list(AuthOperationOptions::default()).await.unwrap();
        assert_eq!(
            listed.into_iter().map(|entry| entry.provider_id).collect::<Vec<_>>(),
            ["openai", "google"]
        );
        assert_eq!(
            storage
                .read("anthropic", AuthOperationOptions::default())
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            storage.read("openai", AuthOperationOptions::default()).await.unwrap(),
            Some(api_key("openai-key"))
        );
        assert_eq!(
            storage.read("google", AuthOperationOptions::default()).await.unwrap(),
            Some(api_key("external-key"))
        );
    });
}

#[test]
fn in_memory_storage_implements_the_same_behavior() {
    let _guard = serial();
    let mut data = Map::new();
    data.insert("anthropic".to_owned(), json!({"type": "api_key", "key": "initial"}));
    let storage = AuthStorage::in_memory(data);

    runtime().block_on(async {
        assert_eq!(
            storage
                .read("anthropic", AuthOperationOptions::default())
                .await
                .unwrap(),
            Some(api_key("initial"))
        );
        storage
            .modify(
                "anthropic",
                Arc::new(|_current| Box::pin(async { Ok(Some(api_key("updated"))) })),
                AuthOperationOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            storage
                .read("anthropic", AuthOperationOptions::default())
                .await
                .unwrap(),
            Some(api_key("updated"))
        );
        storage
            .delete("anthropic", AuthOperationOptions::default())
            .await
            .unwrap();
        assert!(storage.list(AuthOperationOptions::default()).await.unwrap().is_empty());
    });
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
            self.inner.with_lock(operation)
        }
        fn with_lock_async(
            &self,
            operation: AsyncLockOperation,
            options: AuthOperationOptions,
        ) -> BoxFuture<'_, Result<(), StorageError>> {
            if self.failed.fetch_add(1, Ordering::SeqCst) == 0 {
                return Box::pin(async { Err(StorageError::new("lock unavailable")) });
            }
            self.inner.with_lock_async(operation, options)
        }
    }

    let backend = Arc::new(FailingOnce {
        inner: FileAuthStorageBackend::new(&JsString::from(&auth_path.display().to_string())),
        failed: AtomicUsize::new(0),
    });
    let storage = AuthStorage::from_storage(backend);

    let error = runtime()
        .block_on(storage.modify(
            "openai",
            Arc::new(|_current| Box::pin(async { Ok(Some(api_key("new"))) })),
            AuthOperationOptions::default(),
        ))
        .unwrap_err();
    assert!(error.to_string().contains("lock unavailable"), "error: {error:?}");
    let stored: Value = serde_json::from_str(&fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert_eq!(stored, json!({"anthropic": {"type": "api_key", "key": "stored"}}));

    runtime()
        .block_on(storage.modify(
            "openai",
            Arc::new(|_current| Box::pin(async { Ok(Some(api_key("new"))) })),
            AuthOperationOptions::default(),
        ))
        .unwrap();
    let stored: Value = serde_json::from_str(&fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert_eq!(
        stored,
        json!({"anthropic": {"type": "api_key", "key": "stored"}, "openai": {"type": "api_key", "key": "new"}})
    );
}

#[test]
fn pre_aborted_operations_do_not_create_the_file_or_run_the_mutation() {
    let _guard = serial();
    let dir = temp_dir("pre-aborted");
    let auth_path = dir.join("auth.json");
    let backend = FileAuthStorageBackend::new(&JsString::from(&auth_path.display().to_string()));
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    controller_signal.abort(None);
    let ran = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&ran);

    let error = runtime()
        .block_on(backend.with_lock_async(
            Box::new(move |_current| {
                let counter = Arc::clone(&counter);
                Box::pin(async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(Some("{}".to_owned()))
                })
            }),
            AuthOperationOptions::new(Some(signal)),
        ))
        .unwrap_err();
    assert!(error.is_aborted(), "error: {error:?}");
    assert_eq!(ran.load(Ordering::SeqCst), 0);
    assert!(!auth_path.exists());
}

#[test]
fn aborts_while_waiting_for_a_held_file_lock_without_running_the_mutation() {
    let _guard = serial();
    let dir = temp_dir("wait-abort");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": "stored"}}));
    let backend = FileAuthStorageBackend::new(&JsString::from(&auth_path.display().to_string()));
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let ran = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&ran);

    runtime().block_on(async {
        let holder = crate::lock_holder(&auth_path).await;
        let pending = backend.with_lock_async(
            Box::new(move |_current| {
                let counter = Arc::clone(&counter);
                Box::pin(async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok(Some("{}".to_owned()))
                })
            }),
            AuthOperationOptions::new(Some(signal.clone())),
        );
        tokio::pin!(pending);
        tokio::select! {
            result = &mut pending => panic!("must not finish while the lock is held: {result:?}"),
            () = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
        controller_signal.abort(None);
        let error = pending.await.unwrap_err();
        assert!(error.is_aborted(), "error: {error:?}");
        holder.release().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    assert_eq!(ran.load(Ordering::SeqCst), 0);
    let stored: Value = serde_json::from_str(&fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert_eq!(stored, json!({"anthropic": {"type": "api_key", "key": "stored"}}));
}

fn lock_holder(path: &std::path::Path) -> BoxFuture<'static, pi_coding_agent::utils::lockfile::LockGuard> {
    let path = path.to_owned();
    Box::pin(async move {
        pi_coding_agent::utils::lockfile::acquire(
            &path,
            &pi_coding_agent::utils::lockfile::LockOptions {
                realpath: false,
                ..Default::default()
            },
        )
        .await
        .expect("holder must acquire")
    })
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
        let storage = AuthStorage::create(Some(&auth_path.to_str().unwrap().into()));
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
    let credential = read_stored_credential("anthropic", Some(&JsString::from(&auth_path.display().to_string())));
    let Credential::ApiKey(api_key) = credential.expect("credential") else {
        panic!("expected api key");
    };
    assert_eq!(api_key.key.as_ref(), Some(&JsString::from("$RAW")));
    assert_eq!(
        read_stored_credential("missing", Some(&JsString::from(&auth_path.display().to_string()))),
        None
    );
    assert_eq!(
        read_stored_credential("anthropic", Some(&JsString::from("/nonexistent/auth.json"))),
        None
    );
}

#[test]
fn auth_files_are_private() {
    let _guard = serial();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("modes");
        let auth_path = dir.join("nested/auth.json");
        let storage = AuthStorage::create(Some(&JsString::from(&auth_path.display().to_string())));
        runtime()
            .block_on(storage.delete("missing", AuthOperationOptions::default()))
            .unwrap();
        let file_mode = fs::metadata(&auth_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600);
        let dir_mode = fs::metadata(auth_path.parent().unwrap()).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
    }
}

#[test]
fn in_memory_backend_rejects_aborted_operations() {
    let _guard = serial();
    let backend = InMemoryAuthStorageBackend::new();
    backend.set_value(Some("{}".to_owned()));
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    controller_signal.abort(None);
    let error = runtime()
        .block_on(backend.with_lock_async(
            Box::new(|_current| Box::pin(async { Ok(None) })),
            AuthOperationOptions::new(Some(signal)),
        ))
        .unwrap_err();
    assert!(matches!(error, StorageError { .. }) && error.is_aborted());
    let _ = AuthError::Aborted;
}
