//! Ports `packages/coding-agent/test/footer-data-provider.test.ts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use pi_ai::request::BoxFuture;
use pi_coding_agent::core::footer_data_provider::{
    FooterDataProvider, GitBranchRunner, find_git_paths, normalize_footer_executable_name,
    resolve_footer_executable_name_with_env,
};

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-footer-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn create_plain_repo(temp: &Path) -> PathBuf {
    let repo = temp.join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    repo
}

fn create_plain_reftable_repo(temp: &Path) -> PathBuf {
    let repo = temp.join("repo");
    fs::create_dir_all(repo.join(".git/reftable")).unwrap();
    fs::write(repo.join(".git/HEAD"), "ref: refs/heads/.invalid\n").unwrap();
    repo
}

struct WorktreeFixture {
    worktree_dir: PathBuf,
    reftable_dir: PathBuf,
}

fn create_reftable_worktree(temp: &Path) -> WorktreeFixture {
    let repo = temp.join("repo");
    let common_git_dir = repo.join(".git");
    let git_dir = common_git_dir.join("worktrees/src");
    let worktree_dir = temp.join("worktree");
    let reftable_dir = common_git_dir.join("reftable");
    fs::create_dir_all(&git_dir).unwrap();
    fs::create_dir_all(&reftable_dir).unwrap();
    fs::create_dir_all(&worktree_dir).unwrap();
    fs::write(worktree_dir.join(".git"), format!("gitdir: {}\n", git_dir.display())).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/.invalid\n").unwrap();
    fs::write(git_dir.join("commondir"), "../..\n").unwrap();
    fs::write(reftable_dir.join("tables.list"), "0\n").unwrap();
    WorktreeFixture {
        worktree_dir,
        reftable_dir,
    }
}

#[derive(Default)]
struct StubGit {
    sync_calls: AtomicUsize,
    async_calls: AtomicUsize,
    branch: Mutex<Option<String>>,
}

impl StubGit {
    fn new(branch: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            sync_calls: AtomicUsize::new(0),
            async_calls: AtomicUsize::new(0),
            branch: Mutex::new(branch.map(str::to_owned)),
        })
    }

    fn set_branch(&self, branch: Option<&str>) {
        *self.branch.lock().unwrap() = branch.map(str::to_owned);
    }

    fn branch(&self) -> Option<String> {
        self.branch.lock().unwrap().clone()
    }
}

impl GitBranchRunner for StubGit {
    fn resolve_sync(&self, _repo_dir: &Path) -> Option<String> {
        self.sync_calls.fetch_add(1, Ordering::SeqCst);
        self.branch()
    }

    fn resolve_async(&self, _repo_dir: &Path) -> BoxFuture<'static, Option<String>> {
        self.async_calls.fetch_add(1, Ordering::SeqCst);
        let branch = self.branch();
        Box::pin(async move { branch })
    }
}

async fn wait_for(condition: impl Fn() -> bool) {
    let started = std::time::Instant::now();
    while !condition() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timed out waiting for condition"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
fn footer_executable_name_omits_the_default_pi() {
    assert_eq!(normalize_footer_executable_name("pi"), None);
    assert_eq!(normalize_footer_executable_name("pi-dev"), Some("pi-dev".to_owned()));
    assert_eq!(
        normalize_footer_executable_name("pi-test.sh"),
        Some("pi-test.sh".to_owned())
    );
    assert_eq!(normalize_footer_executable_name("node"), None);
    assert_eq!(normalize_footer_executable_name(""), None);
}

#[test]
fn footer_executable_name_prefers_the_wrapper_value() {
    assert_eq!(
        resolve_footer_executable_name_with_env(&["/repo/pi-test.sh"], Some("pi-dev")),
        Some("pi-dev".to_owned())
    );
    assert_eq!(
        resolve_footer_executable_name_with_env(&["/usr/bin/pi"], Some("")),
        None
    );
    assert_eq!(
        resolve_footer_executable_name_with_env(&["/usr/bin/pi-dev"], None),
        Some("pi-dev".to_owned())
    );
}

#[test]
fn find_git_paths_reads_regular_and_worktree_layouts() {
    let temp = temp_dir("paths");
    let repo = create_plain_repo(&temp);
    let nested = repo.join("src/nested");
    fs::create_dir_all(&nested).unwrap();
    let paths = find_git_paths(&nested).unwrap();
    assert_eq!(paths.repo_dir, repo);
    assert_eq!(paths.common_git_dir, repo.join(".git"));
    assert_eq!(paths.head_path, repo.join(".git/HEAD"));

    let fixture = create_reftable_worktree(&temp);
    let paths = find_git_paths(&fixture.worktree_dir).unwrap();
    assert_eq!(paths.repo_dir, fixture.worktree_dir);
    assert_eq!(paths.common_git_dir, temp.join("repo/.git"));
    assert_eq!(paths.head_path, temp.join("repo/.git/worktrees/src/HEAD"));

    fs::remove_dir_all(temp).unwrap();
}

#[tokio::test]
async fn uses_head_directly_in_a_regular_repo_from_a_nested_directory() {
    let temp = temp_dir("regular");
    let repo = create_plain_repo(&temp);
    let nested = repo.join("src/nested");
    fs::create_dir_all(&nested).unwrap();
    let git = StubGit::new(Some("ignored"));
    let provider = FooterDataProvider::with_git_runner(&nested, git.clone());
    assert_eq!(provider.get_git_branch().as_deref(), Some("main"));
    assert_eq!(git.sync_calls.load(Ordering::SeqCst), 0);
    provider.dispose();
    fs::remove_dir_all(temp).unwrap();
}

#[tokio::test]
async fn resolves_the_branch_via_git_when_head_is_invalid() {
    let temp = temp_dir("reftable");
    let repo = create_plain_reftable_repo(&temp);
    let git = StubGit::new(Some("main"));
    let provider = FooterDataProvider::with_git_runner(&repo, git.clone());
    assert_eq!(provider.get_git_branch().as_deref(), Some("main"));
    assert_eq!(git.sync_calls.load(Ordering::SeqCst), 1);
    provider.dispose();

    git.set_branch(None);
    let provider = FooterDataProvider::with_git_runner(&repo, git.clone());
    assert_eq!(provider.get_git_branch().as_deref(), Some("detached"));
    provider.dispose();
    fs::remove_dir_all(temp).unwrap();
}

#[tokio::test]
async fn resolves_the_branch_via_git_in_a_reftable_worktree() {
    let temp = temp_dir("worktree");
    let fixture = create_reftable_worktree(&temp);
    let git = StubGit::new(Some("main"));
    let provider = FooterDataProvider::with_git_runner(&fixture.worktree_dir, git.clone());
    assert_eq!(provider.get_git_branch().as_deref(), Some("main"));
    provider.dispose();
    fs::remove_dir_all(temp).unwrap();
}

#[tokio::test]
async fn does_not_notify_listeners_when_reftable_updates_keep_the_same_branch() {
    let temp = temp_dir("same-branch");
    let fixture = create_reftable_worktree(&temp);
    let git = StubGit::new(Some("main"));
    let provider = FooterDataProvider::with_git_runner(&fixture.worktree_dir, git.clone());
    assert_eq!(provider.get_git_branch().as_deref(), Some("main"));

    let notifications = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&notifications);
    let _unsubscribe = provider.on_branch_change(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    fs::write(fixture.reftable_dir.join("tables.list"), "1\n").unwrap();
    wait_for(|| git.async_calls.load(Ordering::SeqCst) >= 1).await;
    tokio::time::sleep(Duration::from_millis(700)).await;

    assert_eq!(git.async_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider.get_git_branch().as_deref(), Some("main"));
    assert_eq!(notifications.load(Ordering::SeqCst), 0);
    provider.dispose();
    fs::remove_dir_all(temp).unwrap();
}

#[tokio::test]
async fn debounces_rapid_reftable_updates_into_a_single_async_refresh() {
    let temp = temp_dir("debounce");
    let fixture = create_reftable_worktree(&temp);
    let git = StubGit::new(Some("main"));
    let provider = FooterDataProvider::with_git_runner(&fixture.worktree_dir, git.clone());
    assert_eq!(provider.get_git_branch().as_deref(), Some("main"));

    fs::write(fixture.reftable_dir.join("tables.list"), "1\n").unwrap();
    fs::write(fixture.reftable_dir.join("tables.list"), "2\n").unwrap();
    fs::write(fixture.reftable_dir.join("tables.list"), "3\n").unwrap();
    wait_for(|| git.async_calls.load(Ordering::SeqCst) >= 1).await;
    tokio::time::sleep(Duration::from_millis(700)).await;

    assert_eq!(git.async_calls.load(Ordering::SeqCst), 1);
    provider.dispose();
    fs::remove_dir_all(temp).unwrap();
}

#[tokio::test]
async fn updates_the_cached_branch_when_the_reftable_directory_changes() {
    let temp = temp_dir("branch-change");
    let fixture = create_reftable_worktree(&temp);
    let git = StubGit::new(Some("main"));
    let provider = FooterDataProvider::with_git_runner(&fixture.worktree_dir, git.clone());
    assert_eq!(provider.get_git_branch().as_deref(), Some("main"));

    git.set_branch(Some("foo"));
    let notifications = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&notifications);
    let _unsubscribe = provider.on_branch_change(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    fs::write(fixture.reftable_dir.join("tables.list"), "1\n").unwrap();
    wait_for(|| git.async_calls.load(Ordering::SeqCst) >= 1).await;
    wait_for(|| provider.get_git_branch().as_deref() == Some("foo")).await;

    assert_eq!(git.async_calls.load(Ordering::SeqCst), 1);
    assert_eq!(notifications.load(Ordering::SeqCst), 1);
    provider.dispose();
    fs::remove_dir_all(temp).unwrap();
}

#[tokio::test]
async fn set_cwd_recomputes_the_branch_and_notifies() {
    let temp = temp_dir("set-cwd");
    let first = create_plain_repo(&temp);
    let second = temp.join("second");
    fs::create_dir_all(second.join(".git")).unwrap();
    fs::write(second.join(".git/HEAD"), "ref: refs/heads/other\n").unwrap();

    let git = StubGit::new(Some("main"));
    let provider = FooterDataProvider::with_git_runner(&first, git);
    assert_eq!(provider.get_git_branch().as_deref(), Some("main"));

    let notifications = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&notifications);
    let _unsubscribe = provider.on_branch_change(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    provider.set_cwd(&second);
    assert_eq!(provider.get_git_branch().as_deref(), Some("other"));
    assert_eq!(notifications.load(Ordering::SeqCst), 1);
    provider.dispose();
    fs::remove_dir_all(temp).unwrap();
}

#[tokio::test]
async fn tracks_extension_statuses_and_provider_count() {
    let temp = temp_dir("statuses");
    let repo = create_plain_repo(&temp);
    let provider = FooterDataProvider::with_git_runner(&repo, StubGit::new(None));

    provider.set_extension_status("a", Some("one"));
    provider.set_extension_status("b", Some("two"));
    assert_eq!(provider.get_extension_statuses().len(), 2);
    provider.set_extension_status("a", None);
    assert_eq!(
        provider.get_extension_statuses().get("b").map(String::as_str),
        Some("two")
    );
    provider.clear_extension_statuses();
    assert!(provider.get_extension_statuses().is_empty());

    provider.set_available_provider_count(3);
    assert_eq!(provider.get_available_provider_count(), 3);
    provider.dispose();
    fs::remove_dir_all(temp).unwrap();
}
