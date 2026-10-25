use super::tests::{CountingGit, snapshot};
use super::*;

struct WatchRecord {
    path: PathBuf,
    active: Arc<AtomicBool>,
    listener: Mutex<FooterWatchListener>,
    failed: FooterWatchFailure,
}
struct WatchLease(Arc<AtomicBool>);
impl Drop for WatchLease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
fn emit(records: &Mutex<Vec<Arc<WatchRecord>>>, path: &Path, error: bool) {
    for record in lock(records).clone() {
        if record.path == path && record.active.load(Ordering::SeqCst) {
            if error {
                (record.failed)();
            } else {
                (lock(&record.listener))(WatchEventKind::Change, None);
            }
        }
    }
}

#[tokio::test]
async fn unproducible_runtime_watch_error_clears_delivery_and_retries_after_the_frozen_delay() {
    let mut source = crate::test_oracle::TsOracle::spawn(
        "footer-watch-oracle.mts",
        "packages/coding-agent/src/core/footer-data-provider.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"operation":"retry"}));
    let root = tempfile::tempdir().unwrap();
    let git_dir = root.path().join(".git");
    let table = git_dir.join("reftable");
    std::fs::create_dir_all(&table).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/.invalid\n").unwrap();
    std::fs::write(table.join("tables.list"), "0\n").unwrap();
    let records = Arc::new(Mutex::new(Vec::<Arc<WatchRecord>>::new()));
    let factory_records = records.clone();
    let factory: FooterWatchFactory = Arc::new(move |path, listener, failed| {
        let active = Arc::new(AtomicBool::new(true));
        lock(&factory_records).push(Arc::new(WatchRecord {
            path: path.into(),
            active: active.clone(),
            listener: Mutex::new(listener),
            failed,
        }));
        Some(Box::new(WatchLease(active)))
    });
    let git = Arc::new(CountingGit {
        branch: Mutex::new("main".into()),
        executions: AtomicUsize::new(0),
    });
    let provider = FooterDataProvider::with_git_and_watch_factory(root.path().to_str().unwrap(), git.clone(), factory);
    let changes = Arc::new(Mutex::new(Vec::new()));
    let callback_changes = changes.clone();
    let callback_provider = provider.clone();
    let _subscription = provider.on_branch_change(move || {
        lock(&callback_changes).push(callback_provider.get_git_branch());
        Ok(())
    });
    let initial = provider.get_git_branch();
    tokio::time::sleep(Duration::from_millis(50)).await;
    emit(&records, &git_dir, true);
    *lock(&git.branch) = "next".into();
    std::fs::write(table.join("tables.list"), "1\n").unwrap();
    emit(&records, &table, false);
    emit(&records, &table.join("tables.list"), false);
    tokio::time::sleep(Duration::from_millis(800)).await;
    let before = snapshot(&provider, &changes, &git);
    tokio::time::sleep(Duration::from_millis(4400)).await;
    emit(&records, &table, false);
    tokio::time::sleep(Duration::from_millis(800)).await;
    let after = snapshot(&provider, &changes, &git);
    provider.dispose();
    *lock(&git.branch) = "disposed".into();
    emit(&records, &table, false);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let disposed = snapshot(&provider, &changes, &git);
    lock(&records).clear();
    assert_eq!(
        pi_js_compat::js_json!({"initial":initial,"before":before,"after":after,"disposed":disposed}),
        expected
    );
}
