//! Frozen auth-storage callback/write boundaries, using disposable credentials only.
use pi_ai::auth::types::AuthOperationOptions;
use pi_coding_agent::core::auth_storage::{AuthStorageBackend, FileAuthStorageBackend};
use pi_js_compat::js_json::JsString;
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "auth-phase1-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&path).unwrap();
    let file = path.join("auth.json");
    fs::write(
        &file,
        json!({"fake":{"type":"oauth","refresh":"A","access":"old","expires":0},"unrelated":{"sentinel":[1,2,3]}})
            .to_string(),
    )
    .unwrap();
    file
}

#[cfg(unix)]
#[tokio::test]
async fn auth_heartbeat_permission_error_uses_auth_thirty_second_threshold() {
    use std::os::unix::fs::PermissionsExt;
    let file = fixture("transient-stat");
    let parent = file.parent().unwrap().to_owned();
    let backend = FileAuthStorageBackend::new(&JsString::from(file.to_str().unwrap())).unwrap();
    let result = backend
        .with_lock_async(
            Box::new(move |_| {
                Box::pin(async move {
                    let lock_path = parent.join("auth.json.lock");
                    let before = fs::metadata(&lock_path).unwrap().modified().unwrap();
                    fs::set_permissions(&parent, fs::Permissions::from_mode(0o0)).unwrap();
                    // Owned inaccessible parent makes the first heartbeat stat fail, but it is not yet stale.
                    tokio::time::sleep(Duration::from_millis(16_100)).await;
                    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
                    tokio::time::sleep(Duration::from_millis(1_100)).await;
                    assert!(
                        fs::metadata(&lock_path).unwrap().modified().unwrap() > before,
                        "transient heartbeat must retry after 1s, not wait another 15s"
                    );
                    Ok(None)
                })
            }),
            AuthOperationOptions::default(),
        )
        .await;
    assert!(
        result.is_ok(),
        "transient error before auth stale deadline was compromised: {result:?}"
    );
}

#[tokio::test]
async fn compromised_callback_cannot_write_under_lost_lock() {
    let file = fixture("compromised");
    let backend = FileAuthStorageBackend::new(&JsString::from(file.to_str().unwrap())).unwrap();
    let lock_path = PathBuf::from(format!("{}.lock", file.display()));
    let outcome = backend
        .with_lock_async(
            Box::new(move |_| {
                Box::pin(async move {
                    fs::remove_dir(&lock_path).unwrap();
                    tokio::time::sleep(Duration::from_millis(15_100)).await;
                    Ok(Some("{\"unsafe\":true}".to_owned()))
                })
            }),
            AuthOperationOptions::default(),
        )
        .await;
    assert!(outcome.is_err(), "backend wrote after heartbeat detected compromise");
    assert!(fs::read_to_string(file).unwrap().contains("unrelated"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_auth_contender_cannot_steal_three_second_live_async_lock() {
    let file = fixture("live");
    let backend = Arc::new(FileAuthStorageBackend::new(&JsString::from(file.to_str().unwrap())).unwrap());
    let holder_backend = backend.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let holder = tokio::spawn(async move {
        holder_backend
            .with_lock_async(
                Box::new(move |_| {
                    Box::pin(async move {
                        ready_tx.send(()).unwrap();
                        release_rx.await.unwrap();
                        Ok(None)
                    })
                }),
                AuthOperationOptions::default(),
            )
            .await
    });
    ready_rx.await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let contender = backend.with_lock(Box::new(|_| Ok(Some("{\"stolen\":true}".to_owned()))));
    release_tx.send(()).unwrap();
    assert!(contender.is_err(), "sync caller stole healthy async holder");
    holder.await.unwrap().unwrap();
    assert!(fs::read_to_string(file).unwrap().contains("unrelated"));
}
