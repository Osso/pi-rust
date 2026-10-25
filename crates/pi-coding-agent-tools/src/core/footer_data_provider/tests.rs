use super::*;
use pi_js_compat::{js_json, js_json::JsValue};
use std::os::unix::fs::PermissionsExt;

struct CountingGit {
    branch: Mutex<JsString>,
    executions: AtomicUsize,
}
impl GitBranchRunner for CountingGit {
    fn resolve_sync(&self, _: &JsString) -> Option<JsString> {
        Some(lock(&self.branch).clone())
    }
    fn resolve_async(&self, _: &JsString) -> BoxFuture<'static, Option<JsString>> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        let branch = lock(&self.branch).clone();
        Box::pin(async move { Some(branch) })
    }
}

fn snapshot(provider: &FooterDataProvider, changes: &Mutex<Vec<Option<JsString>>>, git: &CountingGit) -> JsValue {
    js_json!({"branch":provider.get_git_branch(),"changes":*lock(changes),"executions":git.executions.load(Ordering::SeqCst)})
}

async fn native_real_watch_scenario(operation: &str) -> JsValue {
    let root = tempfile::tempdir().unwrap();
    let git_dir = root.path().join(".git");
    let table = git_dir.join("reftable");
    let list = table.join("tables.list");
    std::fs::create_dir_all(&table).unwrap();
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/.invalid\n").unwrap();
    std::fs::write(&list, "0\n").unwrap();
    let git = Arc::new(CountingGit {
        branch: Mutex::new("main".into()),
        executions: AtomicUsize::new(0),
    });
    let provider = FooterDataProvider::with_git_runner(root.path().to_str().unwrap(), git.clone());
    let changes = Arc::new(Mutex::new(Vec::new()));
    let callback_changes = changes.clone();
    let callback_provider = provider.clone();
    let _subscription = provider.on_branch_change(move || {
        lock(&callback_changes).push(callback_provider.get_git_branch());
        Ok(())
    });
    let initial = provider.get_git_branch();
    let permission_path = match operation {
        "head_fail" => Some(&git_dir),
        "reftable_fail" => Some(&table),
        _ => None,
    };
    if let Some(path) = permission_path {
        provider.clear_git_watchers();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o111)).unwrap();
        provider.setup_git_watcher();
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    *lock(&git.branch) = "next".into();
    std::fs::write(&list, "1\n").unwrap();
    tokio::time::sleep(Duration::from_millis(if operation == "poll" { 1800 } else { 800 })).await;
    let before = snapshot(&provider, &changes, &git);
    provider.dispose();
    *lock(&git.branch) = "disposed".into();
    tokio::time::sleep(Duration::from_millis(600)).await;
    let disposed = snapshot(&provider, &changes, &git);
    if let Some(path) = permission_path {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    js_json!({"initial":initial,"before":before,"after":null,"disposed":disposed})
}

#[tokio::test]
async fn real_watch_failures_stop_dependent_watchers_and_tables_have_no_extra_polling() {
    let mut source = crate::test_oracle::TsOracle::spawn(
        "footer-watch-oracle.mts",
        "packages/coding-agent/src/core/footer-data-provider.ts",
    );
    let mut expected_cases = Vec::new();
    let mut actual_cases = Vec::new();
    for operation in ["head_fail", "reftable_fail", "poll"] {
        let expected = source.request(&js_json!({"operation":operation}));
        let actual = native_real_watch_scenario(operation).await;
        expected_cases.push(js_json!({"operation":operation,"result":expected}));
        actual_cases.push(js_json!({"operation":operation,"result":actual}));
    }
    assert_eq!(actual_cases, expected_cases);
}
