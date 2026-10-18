use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use pi_coding_agent::utils::lockfile::{LockErrorCode, LockOptions, acquire, acquire_sync, check};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pi-lockfile-{name}-{}-{:?}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn lock_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
}

#[test]
fn acquire_creates_a_lock_directory_and_release_removes_it() {
    let dir = temp_dir("acquire");
    let target = dir.join("auth.json");
    fs::write(&target, "{}").unwrap();

    let guard = acquire_sync(&target, &LockOptions::default()).unwrap();
    let lock = lock_path(&target);
    assert!(lock.is_dir(), "lock must be a directory");
    let modified = fs::metadata(&lock).unwrap().modified().unwrap();
    let ahead = modified
        .duration_since(SystemTime::now())
        .map(|ahead| ahead.as_millis())
        .unwrap_or(0);
    assert!(ahead > 0, "probe sets the mtime slightly into the future");

    guard.release().unwrap();
    assert!(!lock.exists());
}

#[test]
fn a_held_lock_reports_elocked_and_keeps_the_directory() {
    let dir = temp_dir("held");
    let target = dir.join("trust.json");
    let guard = acquire_sync(&target, &LockOptions::default()).unwrap();

    let error = acquire_sync(&target, &LockOptions::default()).expect_err("second lock must fail");
    assert_eq!(error.code, LockErrorCode::Locked);
    assert_eq!(error.to_string(), "Lock file is already being held");
    assert_eq!(error.file.as_deref(), Some(lock_path(&target).as_path()));
    assert!(lock_path(&target).is_dir());

    guard.release().unwrap();
    assert!(acquire_sync(&target, &LockOptions::default()).is_ok());
}

#[test]
fn a_stale_lock_directory_is_removed_and_reacquired() {
    let dir = temp_dir("stale");
    let target = dir.join("settings.json");
    let lock = lock_path(&target);
    fs::create_dir_all(&lock).unwrap();
    let old = SystemTime::now() - Duration::from_secs(5);
    fs::File::open(&lock).unwrap().set_modified(old).unwrap();

    let options = LockOptions {
        stale_ms: 1_000,
        ..LockOptions::default()
    };
    let guard = acquire_sync(&target, &options).expect("stale lock must be reacquired");
    assert!(lock.is_dir());
    guard.release().unwrap();
}

#[test]
fn retries_wait_for_the_holder_to_release() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = temp_dir("retry");
        let target = dir.join("models-store.json");
        let holder = acquire_sync(&target, &LockOptions::default()).unwrap();
        let release_holder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            holder.release().unwrap();
        });

        let started = SystemTime::now();
        let options = LockOptions {
            retries: 5,
            stale_ms: 10_000,
            ..LockOptions::default()
        };
        let guard = acquire(&target, &options)
            .await
            .expect("retry must acquire after release");
        let waited = started.elapsed().unwrap();
        assert!(waited >= Duration::from_millis(250), "waited {waited:?}");
        guard.release().unwrap();
        release_holder.join().unwrap();
    });
}

#[test]
fn custom_lockfile_path_and_check_report_lock_state() {
    let dir = temp_dir("check");
    let target = dir.join("auth.json");
    let custom = dir.join("custom.lock");
    let options = LockOptions {
        lockfile_path: Some(custom.clone()),
        ..LockOptions::default()
    };

    assert!(!check(&target, &options).unwrap());
    let guard = acquire_sync(&target, &options).unwrap();
    assert!(check(&target, &options).unwrap());
    assert!(custom.is_dir());
    assert!(!lock_path(&target).exists());
    guard.release().unwrap();
    assert!(!check(&target, &options).unwrap());
}

#[test]
fn sync_acquisition_rejects_retries() {
    let dir = temp_dir("sync-retries");
    let target = dir.join("auth.json");
    let options = LockOptions {
        retries: 2,
        ..LockOptions::default()
    };
    let error = acquire_sync(&target, &options).expect_err("sync retries must be rejected");
    assert_eq!(error.code, LockErrorCode::Sync);
    assert_eq!(error.to_string(), "Cannot use retries with the sync api");
}

#[test]
fn async_acquisition_keeps_the_mtime_fresh_and_reports_compromise() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = temp_dir("refresh");
        let target = dir.join("auth.json");
        let options = LockOptions {
            stale_ms: 1_000,
            update_ms: Some(1_000),
            ..LockOptions::default()
        };
        let compromised = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&compromised);
        let guard = acquire(
            &target,
            &LockOptions {
                on_compromised: Some(Arc::new(move |_error| {
                    counter.fetch_add(1, Ordering::SeqCst);
                })),
                ..options.clone()
            },
        )
        .await
        .unwrap();

        tokio::time::sleep(Duration::from_millis(1_600)).await;
        let modified = fs::metadata(lock_path(&target)).unwrap().modified().unwrap();
        assert!(
            modified > SystemTime::now() - Duration::from_millis(1_500),
            "the refresh keeps the lock mtime fresh"
        );
        assert_eq!(compromised.load(Ordering::SeqCst), 0);

        fs::remove_dir_all(lock_path(&target)).unwrap();
        tokio::time::sleep(Duration::from_millis(1_300)).await;
        assert_eq!(
            compromised.load(Ordering::SeqCst),
            1,
            "a vanished lock directory compromises the guard"
        );
        let error = guard.release().expect_err("compromised release fails");
        assert_eq!(error.code, LockErrorCode::Released);
        assert_eq!(error.to_string(), "Lock is already released");
    });
}
