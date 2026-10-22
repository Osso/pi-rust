//! Ports `packages/coding-agent/test/models-store.test.ts`.

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use pi_ai::auth::types::AuthOperationOptions;
use pi_ai::models_store::{ModelsStore, ModelsStoreEntry, ModelsStoreOperationOptions};
use pi_ai::request::BoxFuture;
use pi_ai::types::Model;
use pi_coding_agent::core::auth_storage::{
    AsyncLockOperation, AuthStorageBackend, FileAuthStorageBackend, LockOperation, StorageError,
};
use pi_coding_agent::core::models_store::{FileModelsStore, InMemoryCodingAgentModelsStore};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-models-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn model(provider: &str, id: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "api": "openai-completions",
        "provider": provider,
        "baseUrl": "https://example.test/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 1000,
        "maxTokens": 100,
    }))
    .unwrap()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

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
fn persists_provider_catalogs_without_replacing_unrelated_providers() {
    let dir = temp_dir("persist");
    let path = dir.join("models-store.json");
    let store = FileModelsStore::new(Some(&JsString::from(&path.display().to_string())));

    runtime().block_on(async {
        store
            .write(
                "one",
                ModelsStoreEntry {
                    models: vec![model("one", "m1")],
                    last_modified: None,
                    checked_at: Some(100.0),
                    etag: None,
                },
                ModelsStoreOperationOptions::default(),
            )
            .await
            .unwrap();
        store
            .write(
                "two",
                ModelsStoreEntry {
                    models: vec![model("two", "m2")],
                    last_modified: None,
                    checked_at: Some(200.0),
                    etag: None,
                },
                ModelsStoreOperationOptions::default(),
            )
            .await
            .unwrap();
    });

    let reloaded = FileModelsStore::new(Some(&JsString::from(&path.display().to_string())));
    runtime().block_on(async {
        let one = reloaded
            .read("one", ModelsStoreOperationOptions::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            one.models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(),
            ["m1"]
        );
        assert_eq!(one.checked_at, Some(100.0));
        let two = reloaded
            .read("two", ModelsStoreOperationOptions::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            two.models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(),
            ["m2"]
        );

        reloaded
            .delete("one", ModelsStoreOperationOptions::default())
            .await
            .unwrap();
        assert!(
            reloaded
                .read("one", ModelsStoreOperationOptions::default())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reloaded
                .read("two", ModelsStoreOperationOptions::default())
                .await
                .unwrap()
                .is_some()
        );
    });

    let content: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(content.as_object().unwrap().keys().collect::<Vec<_>>(), ["two"]);
}

#[test]
fn coalesces_reloads_across_readers_and_instances() {
    let dir = temp_dir("coalesce");
    let path = dir.join("models-store.json");
    fs::write(
        &path,
        serde_json::to_string(&json!({
            "one": {"models": [serde_json::to_value(model("one", "old")).unwrap()]},
            "two": {"models": [serde_json::to_value(model("two", "m2")).unwrap()]}
        }))
        .unwrap(),
    )
    .unwrap();

    let locks = Arc::new(AtomicUsize::new(0));
    let backend = Arc::new(CountingBackend {
        inner: FileAuthStorageBackend::new(&JsString::from(&path.display().to_string())),
        locks: Arc::clone(&locks),
    });
    let first = FileModelsStore::with_backend(&JsString::from(&path.display().to_string()), backend);
    let second = FileModelsStore::new(Some(&JsString::from(&path.display().to_string())));
    let before = locks.load(Ordering::SeqCst);

    runtime().block_on(async {
        let (one, two, missing) = tokio::join!(
            first.read("one", ModelsStoreOperationOptions::default()),
            second.read("two", ModelsStoreOperationOptions::default()),
            first.read("missing", ModelsStoreOperationOptions::default()),
        );
        assert_eq!(
            one.unwrap()
                .unwrap()
                .models
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["old"]
        );
        assert_eq!(
            two.unwrap()
                .unwrap()
                .models
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["m2"]
        );
        assert!(missing.unwrap().is_none());
    });
    assert_eq!(locks.load(Ordering::SeqCst) - before, 1, "one coalesced reload");
}

#[test]
fn cancels_a_catalog_write_waiting_for_a_held_file_lock() {
    let dir = temp_dir("write-abort");
    let path = dir.join("models-store.json");
    fs::write(
        &path,
        serde_json::to_string(&json!({"one": {"models": [serde_json::to_value(model("one", "existing")).unwrap()]}}))
            .unwrap(),
    )
    .unwrap();
    let store = FileModelsStore::new(Some(&JsString::from(&path.display().to_string())));
    let signal = CancellationToken::new();

    runtime().block_on(async {
        let holder = pi_coding_agent::utils::lockfile::acquire(
            &path,
            &pi_coding_agent::utils::lockfile::LockOptions {
                realpath: false,
                ..Default::default()
            },
        )
        .await
        .expect("holder");
        let pending = store.write(
            "two",
            ModelsStoreEntry {
                models: vec![model("two", "cancelled")],
                last_modified: None,
                checked_at: None,
                etag: None,
            },
            ModelsStoreOperationOptions {
                signal: Some(signal.clone()),
            },
        );
        tokio::pin!(pending);
        tokio::select! {
            result = &mut pending => panic!("must not finish while the lock is held: {result:?}"),
            () = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
        signal.cancel();
        let error = pending.await.unwrap_err();
        assert_eq!(error.to_string(), "The operation was aborted");
        holder.release().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    let stored: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(stored.get("one").is_some());
    assert!(stored.get("two").is_none());
}

#[test]
fn in_memory_store_round_trips_entries() {
    let store = InMemoryCodingAgentModelsStore::new();
    let entry = ModelsStoreEntry {
        models: vec![model("one", "m1")],
        last_modified: Some(5.0),
        checked_at: Some(10.0),
        etag: Some("\"abc\"".to_owned()),
    };
    runtime().block_on(async {
        store
            .write("one", entry.clone(), ModelsStoreOperationOptions::default())
            .await
            .unwrap();
        assert_eq!(
            store.read("one", ModelsStoreOperationOptions::default()).await.unwrap(),
            Some(entry)
        );
        store
            .delete("one", ModelsStoreOperationOptions::default())
            .await
            .unwrap();
        assert_eq!(
            store.read("one", ModelsStoreOperationOptions::default()).await.unwrap(),
            None
        );
    });
}
