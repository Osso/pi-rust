use super::*;
use pi_js_compat::{js_json, js_json::JsValue};
use std::os::unix::fs::PermissionsExt;

pub(super) struct CountingGit {
    pub(super) branch: Mutex<JsString>,
    pub(super) executions: AtomicUsize,
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

pub(super) fn snapshot(
    provider: &FooterDataProvider,
    changes: &Mutex<Vec<Option<JsString>>>,
    git: &CountingGit,
) -> JsValue {
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
    let wsl = operation.starts_with("wsl_");
    let permission_path = match operation {
        "head_fail" | "wsl_empty" | "wsl_ctime" => Some(&git_dir),
        "reftable_fail" => Some(&table),
        _ => None,
    };
    if let Some(path) = permission_path {
        provider.clear_git_watchers();
        if wsl {
            lock(&provider.state.git_paths).as_mut().unwrap().repo_dir = "/mnt/Z/repo".into();
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o111)).unwrap();
        provider.setup_git_watcher();
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    *lock(&git.branch) = "next".into();
    if operation == "wsl_empty" {
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/.invalid\n").unwrap();
    } else if wsl {
        std::fs::set_permissions(git_dir.join("HEAD"), std::fs::Permissions::from_mode(0o600)).unwrap();
    } else {
        std::fs::write(&list, "1\n").unwrap();
    }
    tokio::time::sleep(Duration::from_millis(if operation == "poll" || wsl {
        1800
    } else {
        800
    }))
    .await;
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

#[test]
fn wsl_head_polling_uses_truthiness_and_ctime_instead_of_birthtime() {
    const PROBE: &str = "PI_FOOTER_WSL_PROBE";
    if let Ok(path) = std::env::var(PROBE) {
        let operation = std::env::var("PI_FOOTER_WSL_OPERATION").unwrap();
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(native_real_watch_scenario(&operation));
        std::fs::write(path, pi_js_compat::js_json::stringify_lossless(&result)).unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut source = crate::test_oracle::TsOracle::spawn(
        "footer-watch-oracle.mts",
        "packages/coding-agent/src/core/footer-data-provider.ts",
    );
    let mut expected_cases = Vec::new();
    let mut actual_cases = Vec::new();
    for operation in ["wsl_empty", "wsl_ctime"] {
        let expected = source.request(&js_json!({"operation":operation}));
        let output_path = root.path().join("wsl.json");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "core::footer_data_provider::tests::wsl_head_polling_uses_truthiness_and_ctime_instead_of_birthtime",
                "--nocapture",
            ])
            .env(PROBE, &output_path)
            .env("PI_FOOTER_WSL_OPERATION", operation)
            .env("WSL_DISTRO_NAME", if operation == "wsl_empty" { "" } else { "fixture" })
            .env_remove("WSL_INTEROP")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let actual = pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(output_path).unwrap()).unwrap();
        expected_cases.push(js_json!({"operation":operation,"result":expected}));
        actual_cases.push(js_json!({"operation":operation,"result":actual}));
    }
    assert_eq!(actual_cases, expected_cases);
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
