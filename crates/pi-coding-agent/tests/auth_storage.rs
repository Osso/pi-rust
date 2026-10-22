//! Ports `packages/coding-agent/test/auth-storage.test.ts`.

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use pi_ai::auth::types::{
    ApiKeyCredential, AuthError, AuthOperationOptions, Credential, CredentialStore, CredentialType, OAuthCredential,
};
use pi_ai::request::BoxFuture;
use pi_ai::types::ProviderEnv;
use pi_coding_agent::core::auth_storage::{
    AsyncLockOperation, AuthStorage, AuthStorageBackend, FileAuthStorageBackend, InMemoryAuthStorageBackend,
    LockOperation, ReadOnlyAuthStorage, StorageError, read_stored_credential,
};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

/// `AuthStorage` shares one process-wide read state per path, like the TypeScript module; tests
/// that create stores therefore run one at a time.
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
    assert_eq!(env.get("SCOPED_KEY").map(String::as_str), Some("scoped-value"));
    assert_eq!(env.get("REGION").map(String::as_str), Some("test-region"));
}

#[test]
fn coalesces_file_reloads_across_concurrent_readers_and_instances() {
    let _guard = serial();
    let dir = temp_dir("coalesce");
    let auth_path = dir.join("auth.json");
    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": "old"}}));
    let locks = Arc::new(AtomicUsize::new(0));
    let backend = Arc::new(CountingBackend {
        inner: FileAuthStorageBackend::new(&JsString::from(&auth_path.display().to_string())),
        locks: Arc::clone(&locks),
    });
    let first = AuthStorage::from_storage_at(
        Arc::clone(&backend) as Arc<dyn AuthStorageBackend>,
        Some(auth_path.display().to_string().into()),
    );
    let second = AuthStorage::from_storage_at(
        Arc::clone(&backend) as Arc<dyn AuthStorageBackend>,
        Some(auth_path.display().to_string().into()),
    );
    let locks_before = locks.load(Ordering::SeqCst);

    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "new"}, "openai": {"type": "api_key", "key": "openai-key"}}),
    );

    runtime().block_on(async {
        let (anthropic, openai, credentials) = tokio::join!(
            first.read("anthropic", AuthOperationOptions::default()),
            second.read("openai", AuthOperationOptions::default()),
            first.list(AuthOperationOptions::default()),
        );
        assert_eq!(anthropic.unwrap(), Some(api_key("new")));
        assert_eq!(openai.unwrap(), Some(api_key("openai-key")));
        let ids: Vec<(String, CredentialType)> = credentials
            .unwrap()
            .into_iter()
            .map(|entry| (entry.provider_id, entry.credential_type))
            .collect();
        assert_eq!(
            ids,
            [
                ("anthropic".to_owned(), CredentialType::ApiKey),
                ("openai".to_owned(), CredentialType::ApiKey)
            ]
        );
    });
    assert_eq!(locks.load(Ordering::SeqCst) - locks_before, 1, "one coalesced reload");
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
    assert!(error.to_string().contains("lock unavailable"), "error: {error}");
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
    let signal = CancellationToken::new();
    signal.cancel();
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
    assert!(error.is_aborted(), "error: {error}");
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
    let signal = CancellationToken::new();
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
        signal.cancel();
        let error = pending.await.unwrap_err();
        assert!(error.is_aborted(), "error: {error}");
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
fn read_only_storage_resolves_templates_without_executing_commands() {
    let _guard = serial();
    let dir = temp_dir("read-only");
    let auth_path = dir.join("auth.json");
    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "$NOT_RESOLVED"}}),
    );
    let storage = ReadOnlyAuthStorage::new(Some(&JsString::from(&auth_path.display().to_string())));
    let credential = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap();
    let Credential::ApiKey(api_key) = credential.expect("credential") else {
        panic!("expected api key");
    };
    assert_eq!(api_key.key, None, "unresolved template resolves to undefined");

    write_auth(
        &auth_path,
        json!({"anthropic": {"type": "api_key", "key": "!echo secret"}}),
    );
    let storage = ReadOnlyAuthStorage::new(Some(&JsString::from(&auth_path.display().to_string())));
    let credential = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap();
    let Credential::ApiKey(api_key) = credential.expect("credential") else {
        panic!("expected api key");
    };
    assert_eq!(
        api_key.key.as_ref(),
        Some(&JsString::from("!echo secret")),
        "command references stay literal"
    );

    write_auth(&auth_path, json!({"anthropic": {"type": "api_key", "key": 3}}));
    let storage = ReadOnlyAuthStorage::new(Some(&JsString::from(&auth_path.display().to_string())));
    let error = runtime()
        .block_on(storage.read("anthropic", AuthOperationOptions::default()))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Invalid auth.json credential for provider \"anthropic\""
    );

    write_auth(&auth_path, json!([]));
    let storage = ReadOnlyAuthStorage::new(Some(&JsString::from(&auth_path.display().to_string())));
    let error = runtime()
        .block_on(storage.list(AuthOperationOptions::default()))
        .unwrap_err();
    assert_eq!(error.to_string(), "Invalid auth.json: expected an object");

    let error = runtime()
        .block_on(storage.modify(
            "anthropic",
            Arc::new(|_current| Box::pin(async { Ok(None) })),
            AuthOperationOptions::default(),
        ))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Read-only credential storage cannot modify auth.json"
    );
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
    let signal = CancellationToken::new();
    signal.cancel();
    let error = runtime()
        .block_on(backend.with_lock_async(
            Box::new(|_current| Box::pin(async { Ok(None) })),
            AuthOperationOptions::new(Some(signal)),
        ))
        .unwrap_err();
    assert!(matches!(error, StorageError { .. }) && error.is_aborted());
    let _ = AuthError::Aborted;
}
