//! Source public refresh provenance, callback-return cancellation/write failure/rescue ordering.
use crate::shared_oracle as oracle;
use pi_ai::{
    auth::{AuthError, OAuthCredential},
    oauth::*,
    request::BoxFuture,
};
use pi_coding_agent::core::auth_storage::{
    AsyncLockOperation, AuthStorage, AuthStorageBackend, FileAuthStorageBackend, GetApiKeyOptions, LockOperation,
    StorageError,
};
use pi_js_compat::js_json::JsString;
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
        let rotation = self.count.fetch_add(1, Ordering::SeqCst);
        let fail = self.fail;
        Box::pin(async move {
            if fail {
                return Err(AuthError::failed("server rejected A"));
            };
            assert_eq!(
                current.refresh.to_utf8().unwrap(),
                if rotation == 0 { "A" } else { "B" },
                "fake server rejects spent refresh token"
            );
            let mut next = OAuthCredential::new(if rotation == 0 { "B" } else { "C" }, "new", 4_000_000_000_000.0);
            next.extra.insert("accountId".into(), (&json!("synthetic")).into());
            Ok(next)
        })
    }
    fn get_api_key(
        &self,
        c: &OAuthCredentials,
    ) -> Result<pi_js_compat::js_json::JsString, pi_ai::request::CallbackError> {
        Ok(c.access.clone())
    }
}
struct BoundaryBackend {
    inner: FileAuthStorageBackend,
    fault: &'static str,
    syncs: AtomicUsize,
}
impl AuthStorageBackend for BoundaryBackend {
    fn with_lock(&self, operation: LockOperation<'_>) -> Result<(), StorageError> {
        let n = self.syncs.fetch_add(1, Ordering::SeqCst);
        if (self.fault == "rescue-fails" && n == 1) || (self.fault == "all-sync-fails" && n > 0) {
            return Err(StorageError::new("rescue denied"));
        }
        self.inner.with_lock(operation)
    }
    fn with_lock_async(&self, operation: AsyncLockOperation) -> BoxFuture<'_, Result<(), StorageError>> {
        let fault = self.fault;
        let file = pi_js_compat::js_json::encode_node_utf8(self.inner.auth_path());
        self.inner.with_lock_async(Box::new(move |current| {
            Box::pin(async move {
                let next = operation(current).await?;
                // Barrier is AFTER fake provider result and storage callback, BEFORE native write.
                if fault == "compromise" {
                    fs::remove_dir(format!("{file}.lock")).unwrap();
                    let mut changed: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
                    changed["unrelated"] = json!({"sentinel":"concurrent writer"});
                    changed["new-provider"] = json!({"type":"api_key","key":"concurrent"});
                    fs::write(&file, changed.to_string()).unwrap();
                    tokio::time::sleep(std::time::Duration::from_millis(15_100)).await;
                    return Ok(next);
                }
                Err(StorageError::new("locked write denied"))
            })
        }))
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
fn successful_rotation_callback_then_write_failure_rescues_exact_result() {
    let _serial = SERIAL.lock().unwrap();
    for fault in ["write-fails", "compromise"] {
        let file = fixture();
        let count = Arc::new(AtomicUsize::new(0));
        register_oauth_provider(Arc::new(Fake {
            count: count.clone(),
            fail: false,
        }));
        let storage = AuthStorage::from_storage(Arc::new(BoundaryBackend {
            inner: FileAuthStorageBackend::new(&JsString::from(file.to_str().unwrap())).unwrap(),
            fault,
            syncs: AtomicUsize::new(0),
        }));
        let result = rt()
            .block_on(storage.get_api_key("phase1-rotation", GetApiKeyOptions::default()))
            .unwrap();
        assert_eq!(result, Some(pi_js_compat::js_json::JsString::from("new")));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let fresh: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(fresh["phase1-rotation"]["refresh"], "B");
        assert_eq!(fresh["phase1-rotation"]["accountId"], "synthetic");
        assert_eq!(
            fresh["unrelated"],
            if fault == "compromise" {
                json!({"sentinel":"concurrent writer"})
            } else {
                json!({"sentinel":[1,2,3]})
            }
        );
        assert_eq!(storage.drain_errors().len(), 1);
        let mut peer = oracle::TsOracle::spawn(
            "auth-storage-oracle.ts",
            "packages/coding-agent/src/core/auth-storage.ts",
        );
        let fresh_process = peer.request(&json!({"id":1,"op":"read","authPath":file,"provider":"phase1-rotation"}));
        assert_eq!(fresh_process["ok"], true, "{fresh_process}");
        assert_eq!(fresh_process["output"]["credential"]["refresh"], "B");
        let fresh_store = AuthStorage::create(Some(&JsString::from(file.to_str().unwrap()))).unwrap();
        let mut expired = fresh_store.get("phase1-rotation").unwrap().to_serde().unwrap();
        assert_eq!(expired["refresh"], "B");
        expired["expires"] = json!(0);
        fresh_store.set("phase1-rotation", expired).unwrap();
        assert_eq!(
            rt().block_on(fresh_store.get_api_key("phase1-rotation", GetApiKeyOptions::default()))
                .unwrap()
                .map(|key| key.to_utf8().unwrap())
                .as_deref(),
            Some("new")
        );
        assert_eq!(
            fresh_store.get("phase1-rotation").unwrap().to_serde().unwrap()["refresh"],
            "C"
        );
        assert_eq!(count.load(Ordering::SeqCst), 2);
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
            inner: FileAuthStorageBackend::new(&JsString::from(file.to_str().unwrap())).unwrap(),
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
            "{error:?}"
        );
        let fresh: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(fresh["phase1-rotation"]["refresh"], "A");
        assert_eq!(fresh["unrelated"], json!({"sentinel":[1,2,3]}));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    unregister_oauth_provider("phase1-rotation");
}

#[test]
fn failed_rotation_persistence_and_reload_never_succeed_from_memory() {
    let _serial = SERIAL.lock().unwrap();
    for throws in [true, false] {
        let file = fixture();
        register_oauth_provider(Arc::new(Fake {
            count: Arc::new(AtomicUsize::new(0)),
            fail: false,
        }));
        let backend = Arc::new(BoundaryBackend {
            inner: FileAuthStorageBackend::new(&JsString::from(file.to_str().unwrap())).unwrap(),
            fault: "all-sync-fails",
            syncs: AtomicUsize::new(0),
        });
        let storage = AuthStorage::from_storage(backend.clone());
        let result = rt().block_on(storage.get_api_key(
            "phase1-rotation",
            GetApiKeyOptions {
                throw_on_refresh_failure: throws,
                ..Default::default()
            },
        ));
        let (key, error, cause) = match result {
            Ok(key) => (key, None, None),
            Err(error) => {
                let cause = std::error::Error::source(&error).map(ToString::to_string);
                (None, Some(error.to_string()), cause)
            }
        };
        let expected_error = "Authentication failed for \"phase1-rotation\": rescue denied. Run '/login phase1-rotation' to re-authenticate.";
        assert_eq!(key, None);
        assert_eq!(error.as_deref(), throws.then_some(expected_error));
        assert_eq!(cause.as_deref(), throws.then_some("rescue denied"));
        assert_eq!(backend.syncs.load(Ordering::SeqCst), 3);
        let native = json!({"result":key.map(|key| key.to_utf8().unwrap()),"error":error,"cause":cause,
            "calls":["sync 1","async callback","refresh A->B","after callback: write denied","sync 2","sync 3"],
            "errors":storage.drain_errors().iter().map(ToString::to_string).collect::<Vec<_>>(),
            "memory":pi_js_compat::js_json::JsValue::Object(storage.get_all()).to_serde().unwrap(),
            "disk":serde_json::from_str::<Value>(&fs::read_to_string(&file).unwrap()).unwrap()});
        let request = json!({"id":1,"authPath":file,"throws":throws});
        let mut delta = oracle::TsOracle::spawn("auth-refresh-oracle.ts", &locked_auth_delta());
        let output = delta.request(&request);
        assert_eq!(output["ok"], true, "{output:?}");
        assert_eq!(output["output"], native, "approved delta must have zero discrepancies");
        let mut frozen = oracle::TsOracle::spawn(
            "auth-refresh-oracle.ts",
            "packages/coding-agent/src/core/auth-storage.ts",
        );
        let baseline = frozen.request(&request);
        assert_eq!(baseline["ok"], true, "{baseline}");
        let mut expected_frozen = native.clone();
        expected_frozen["result"] = json!("new");
        expected_frozen["error"] = Value::Null;
        expected_frozen["cause"] = Value::Null;
        assert_eq!(
            baseline["output"], expected_frozen,
            "only approved memory-only-success discrepancy is allowed"
        );
        assert_eq!(
            storage.get("phase1-rotation").unwrap().to_serde().unwrap()["refresh"],
            "B"
        );
        let disk: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(disk["phase1-rotation"]["refresh"], "A");
        assert_eq!(
            native["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            ["locked write denied", "rescue denied", "rescue denied", "rescue denied"]
        );
    }
    unregister_oauth_provider("phase1-rotation");
}

fn locked_auth_delta() -> String {
    use sha2::{Digest, Sha256};
    let lock: Value = serde_json::from_str(include_str!("../../../docs/porting/delta-lock.json")).unwrap();
    let module = "packages/coding-agent/src/core/auth-storage.ts";
    let entry = lock["deltas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["modules"][module].is_string())
        .expect("auth delta must be locked");
    let checkout = entry["checkout"].as_str().unwrap();
    let head = std::process::Command::new("git")
        .args(["-C", checkout, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        entry["commit"].as_str().unwrap()
    );
    let path = std::path::Path::new(checkout).join(module);
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        entry["modules"][module].as_str().unwrap()
    );
    path.to_str().unwrap().to_owned()
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
