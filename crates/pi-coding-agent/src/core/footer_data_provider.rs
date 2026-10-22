//! Port of `packages/coding-agent/src/core/footer-data-provider.ts`.
//!
//! Footer state (git branch, extension statuses, provider count) is data the
//! extension API cannot otherwise reach. Branch detection reads `.git/HEAD`
//! directly and falls back to `git symbolic-ref` for reftable repos, whose HEAD
//! contains the `.invalid` placeholder.

use crate::utils::paths::{node_dirname, node_join, node_normalize, node_resolve};
use indexmap::IndexMap;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::RecommendedWatcher;
use pi_ai::request::BoxFuture;
use pi_ai::types::Model;
use tokio::task::JoinHandle;

use crate::core::session_manager::SessionManager;
use crate::utils::fs_watch::{FS_WATCH_RETRY_DELAY_MS, WatchEventKind, close_watcher, watch_with_error_handler};

/// Debounce for file-system triggered branch refreshes.
const WATCH_DEBOUNCE_MS: u64 = 500;
/// Poll interval for `HEAD` and `reftable/tables.list`.
const GIT_FILE_POLL_INTERVAL_MS: u64 = 1_000;

type BranchChangeCallback = (usize, Arc<dyn Fn() + Send + Sync>);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Git metadata paths for the repository containing a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitPaths {
    pub repo_dir: JsString,
    pub common_git_dir: JsString,
    pub head_path: JsString,
}

/// Estimated context usage shown by the footer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextUsage {
    /// `None` when unknown (for example right after compaction).
    pub tokens: Option<f64>,
    pub context_window: f64,
    pub percent: Option<f64>,
}

/// Session values overriding the footer for one display cycle.
#[derive(Clone)]
pub struct FooterSessionOverride {
    pub cwd: JsString,
    pub session_manager: Option<Arc<Mutex<SessionManager>>>,
    pub model: Option<Model>,
    pub thinking_level: String,
    pub context_usage: Option<ContextUsage>,
}

/// Runs `git symbolic-ref` for reftable repos. Injectable so tests can decide
/// what the `.invalid` placeholder resolves to.
pub trait GitBranchRunner: Send + Sync {
    fn resolve_sync(&self, repo_dir: &JsString) -> Option<String>;
    fn resolve_async(&self, repo_dir: &JsString) -> BoxFuture<'static, Option<String>>;
}

/// Spawns the real `git` executable.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemGitBranchRunner;

impl SystemGitBranchRunner {
    fn command(repo_dir: &JsString) -> std::process::Command {
        let mut command = std::process::Command::new("git");
        command
            .arg("--no-optional-locks")
            .arg("symbolic-ref")
            .arg("--quiet")
            .arg("--short")
            .arg("HEAD")
            .current_dir(encode_node_utf8(repo_dir));
        command
    }
}

impl GitBranchRunner for SystemGitBranchRunner {
    fn resolve_sync(&self, repo_dir: &JsString) -> Option<String> {
        let output = Self::command(repo_dir).output().ok()?;
        if !output.status.success() {
            return None;
        }
        let branch = String::from_utf8(output.stdout).ok()?;
        let branch = branch.trim();
        (!branch.is_empty()).then(|| branch.to_owned())
    }

    fn resolve_async(&self, repo_dir: &JsString) -> BoxFuture<'static, Option<String>> {
        let repo_dir = repo_dir.clone();
        Box::pin(async move {
            let output = tokio::process::Command::new("git")
                .arg("--no-optional-locks")
                .arg("symbolic-ref")
                .arg("--quiet")
                .arg("--short")
                .arg("HEAD")
                .current_dir(encode_node_utf8(&repo_dir))
                .output()
                .await
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let branch = String::from_utf8(output.stdout).ok()?;
            let branch = branch.trim();
            (!branch.is_empty()).then(|| branch.to_owned())
        })
    }
}

/// Finds git metadata by walking up from `cwd`.
pub fn find_git_paths(cwd: &JsString) -> Option<GitPaths> {
    let mut dir = cwd.clone();
    loop {
        let git_path = node_join(&dir, ".git");
        if path_exists(&git_path) {
            match std::fs::metadata(encode_node_utf8(&git_path)) {
                Ok(metadata) if metadata.is_file() => {
                    let content = read_git_text(&git_path)?.trim();
                    if let Some(rest) = content.strip_prefix("gitdir: ") {
                        let git_dir = node_resolve(&dir, rest.trim()).expect("absolute git directory resolves");
                        let head_path = node_join(&git_dir, "HEAD");
                        if !path_exists(&head_path) {
                            return None;
                        }
                        let common_dir_path = node_join(&git_dir, "commondir");
                        let common_git_dir = if path_exists(&common_dir_path) {
                            let value = read_git_text(&common_dir_path)?;
                            node_normalize(
                                node_resolve(&git_dir, value.trim()).expect("absolute common directory resolves"),
                            )
                        } else {
                            git_dir
                        };
                        return Some(GitPaths {
                            repo_dir: dir,
                            common_git_dir,
                            head_path,
                        });
                    }
                }
                Ok(metadata) if metadata.is_dir() => {
                    let head_path = node_join(&git_path, "HEAD");
                    if !path_exists(&head_path) {
                        return None;
                    }
                    return Some(GitPaths {
                        repo_dir: dir,
                        common_git_dir: git_path,
                        head_path,
                    });
                }
                _ => return None,
            }
        }
        let parent = node_dirname(&dir);
        if parent == dir {
            return None;
        }
        dir = parent;
    }
}

fn path_exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}

fn read_git_text(path: &JsString) -> Option<JsString> {
    match std::fs::read(encode_node_utf8(path)) {
        Ok(bytes) => Some(JsString::from(String::from_utf8_lossy(&bytes).as_ref())),
        Err(_) => None, // source catches unreadable git metadata and returns null
    }
}

/// `normalizeFooterExecutableName`: `None` for the default `pi` binary.
pub fn normalize_footer_executable_name(name: &str) -> Option<String> {
    if name.is_empty() || name == "pi" {
        return None;
    }
    let is_pi_variant = name == "pi" || name.starts_with("pi-");
    is_pi_variant.then(|| name.to_owned())
}

fn executable_basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Resolves the executable name shown by the footer.
pub fn resolve_footer_executable_name_with_env(paths: &[&str], configured_name: Option<&str>) -> Option<String> {
    if let Some(configured) = configured_name
        && let Some(name) = normalize_footer_executable_name(configured)
    {
        return Some(name);
    }
    paths
        .iter()
        .filter(|path| !path.is_empty())
        .map(|path| executable_basename(path))
        .find_map(normalize_footer_executable_name)
}

/// Resolves the executable name from `PI_EXECUTABLE_NAME` and `process.argv[1]`/`execPath`.
pub fn resolve_footer_executable_name() -> Option<String> {
    let configured = pi_js_compat::process_env::get("PI_EXECUTABLE_NAME");
    let argv1 = std::env::args_os()
        .nth(1)
        .map(|value| value.to_string_lossy().into_owned());
    let exec_path = std::env::current_exe()
        .ok()
        .map(|value| value.to_string_lossy().into_owned());
    let mut paths = Vec::new();
    if let Some(argv1) = argv1.as_deref() {
        paths.push(argv1);
    }
    if let Some(exec_path) = exec_path.as_deref() {
        paths.push(exec_path);
    }
    resolve_footer_executable_name_with_env(&paths, configured.as_deref())
}

fn is_wsl_environment() -> bool {
    cfg!(target_os = "linux")
        && (pi_js_compat::process_env::get("WSL_DISTRO_NAME").is_some()
            || pi_js_compat::process_env::get("WSL_INTEROP").is_some())
}

fn is_windows_mounted_repo_path(path: &JsString) -> bool {
    let mut characters = path.units().iter().copied();
    if characters.next() != Some(b'/' as u16) {
        return false;
    }
    for expected in [b'm' as u16, b'n' as u16, b't' as u16, b'/' as u16] {
        if characters.next() != Some(expected) {
            return false;
        }
    }
    let Some(drive) = characters.next() else {
        return false;
    };
    if !(drive >= b'a' as u16 && drive <= b'z' as u16 || drive >= b'A' as u16 && drive <= b'Z' as u16) {
        return false;
    }
    characters.next().is_none_or(|unit| unit == b'/' as u16)
}

fn should_poll_git_head(repo_dir: &JsString) -> bool {
    is_wsl_environment() && is_windows_mounted_repo_path(repo_dir)
}

struct FooterState {
    cwd: Mutex<JsString>,
    extension_statuses: Mutex<IndexMap<JsString, JsString>>,
    /// `None` = not computed yet, `Some(None)` = not a repo / detached.
    cached_branch: Mutex<Option<Option<String>>>,
    git_paths: Mutex<Option<GitPaths>>,
    branch_change_callbacks: Mutex<Vec<BranchChangeCallback>>,
    next_callback_id: AtomicUsize,
    available_provider_count: AtomicUsize,
    refresh_timer: Mutex<Option<JoinHandle<()>>>,
    git_watcher_retry_timer: Mutex<Option<JoinHandle<()>>>,
    refresh_in_flight: AtomicBool,
    refresh_pending: AtomicBool,
    disposed: AtomicBool,
    session_override: Mutex<Option<FooterSessionOverride>>,
    head_watcher: Mutex<Option<RecommendedWatcher>>,
    reftable_watcher: Mutex<Option<RecommendedWatcher>>,
    reftable_tables_list_watcher: Mutex<Option<RecommendedWatcher>>,
    poll_timer: Mutex<Option<JoinHandle<()>>>,
    git: Arc<dyn GitBranchRunner>,
}

impl FooterState {
    fn notify_branch_change(&self) {
        let callbacks = lock(&self.branch_change_callbacks).clone();
        for (_, callback) in callbacks {
            callback();
        }
    }

    fn unsubscribe(&self, id: usize) {
        lock(&self.branch_change_callbacks).retain(|(callback_id, _)| *callback_id != id);
    }

    fn resolve_branch_sync(&self) -> Option<String> {
        let paths = lock(&self.git_paths).clone()?;
        let content = std::fs::read_to_string(encode_node_utf8(&paths.head_path)).ok()?;
        let content = content.trim();
        if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
            if branch == ".invalid" {
                return Some(
                    self.git
                        .resolve_sync(&paths.repo_dir)
                        .unwrap_or_else(|| "detached".to_owned()),
                );
            }
            return Some(branch.to_owned());
        }
        Some("detached".to_owned())
    }

    async fn resolve_branch_async(&self) -> Option<String> {
        let paths = lock(&self.git_paths).clone()?;
        let content = std::fs::read_to_string(encode_node_utf8(&paths.head_path)).ok()?;
        let content = content.trim();
        if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
            if branch == ".invalid" {
                return Some(
                    self.git
                        .resolve_async(&paths.repo_dir)
                        .await
                        .unwrap_or_else(|| "detached".to_owned()),
                );
            }
            return Some(branch.to_owned());
        }
        Some("detached".to_owned())
    }
}

/// Provides git branch and extension statuses to the footer.
#[derive(Clone)]
pub struct FooterDataProvider {
    state: Arc<FooterState>,
    runtime: tokio::runtime::Handle,
}

impl FooterDataProvider {
    pub fn new(cwd: impl Into<JsString>) -> Self {
        Self::with_git_runner(cwd, Arc::new(SystemGitBranchRunner))
    }

    pub fn with_git_runner(cwd: impl Into<JsString>, git: Arc<dyn GitBranchRunner>) -> Self {
        let cwd = cwd.into();
        let state = Arc::new(FooterState {
            git_paths: Mutex::new(find_git_paths(&cwd)),
            cwd: Mutex::new(cwd),
            extension_statuses: Mutex::new(IndexMap::new()),
            cached_branch: Mutex::new(None),
            branch_change_callbacks: Mutex::new(Vec::new()),
            next_callback_id: AtomicUsize::new(0),
            available_provider_count: AtomicUsize::new(0),
            refresh_timer: Mutex::new(None),
            git_watcher_retry_timer: Mutex::new(None),
            refresh_in_flight: AtomicBool::new(false),
            refresh_pending: AtomicBool::new(false),
            disposed: AtomicBool::new(false),
            session_override: Mutex::new(None),
            head_watcher: Mutex::new(None),
            reftable_watcher: Mutex::new(None),
            reftable_tables_list_watcher: Mutex::new(None),
            poll_timer: Mutex::new(None),
            git,
        });
        let provider = Self {
            state,
            runtime: tokio::runtime::Handle::current(),
        };
        provider.setup_git_watcher();
        provider
    }

    /// Current branch; `None` when not in a repository, `"detached"` for detached HEAD.
    pub fn get_git_branch(&self) -> Option<String> {
        let mut cached = lock(&self.state.cached_branch);
        if cached.is_none() {
            *cached = Some(self.state.resolve_branch_sync());
        }
        cached.clone().flatten()
    }

    pub fn get_executable_name(&self) -> Option<String> {
        resolve_footer_executable_name()
    }

    pub fn get_extension_statuses(&self) -> IndexMap<JsString, JsString> {
        lock(&self.state.extension_statuses).clone()
    }

    /// Subscribes to branch changes; dropping the subscription unsubscribes.
    pub fn on_branch_change(&self, callback: impl Fn() + Send + Sync + 'static) -> BranchChangeSubscription {
        let id = self.state.next_callback_id.fetch_add(1, Ordering::SeqCst);
        lock(&self.state.branch_change_callbacks).push((id, Arc::new(callback)));
        BranchChangeSubscription {
            state: Arc::clone(&self.state),
            id,
        }
    }

    pub fn get_session_override(&self) -> Option<FooterSessionOverride> {
        lock(&self.state.session_override).clone()
    }

    pub fn set_session_override(&self, value: FooterSessionOverride) {
        *lock(&self.state.session_override) = Some(value);
    }

    pub fn clear_session_override(&self) {
        *lock(&self.state.session_override) = None;
    }

    pub fn set_extension_status(&self, key: &JsString, text: Option<&JsString>) {
        let mut statuses = lock(&self.state.extension_statuses);
        match text {
            Some(text) => {
                statuses.insert(key.to_owned(), text.to_owned());
            }
            None => {
                statuses.shift_remove(key);
            }
        }
    }

    pub fn clear_extension_statuses(&self) {
        lock(&self.state.extension_statuses).clear();
    }

    pub fn get_available_provider_count(&self) -> usize {
        self.state.available_provider_count.load(Ordering::SeqCst)
    }

    pub fn set_available_provider_count(&self, count: usize) {
        self.state.available_provider_count.store(count, Ordering::SeqCst);
    }

    pub fn set_cwd(&self, cwd: impl Into<JsString>) {
        let cwd = cwd.into();
        {
            let mut current = lock(&self.state.cwd);
            if *current == cwd {
                return;
            }
            *current = cwd.clone();
        }
        if let Some(timer) = lock(&self.state.refresh_timer).take() {
            timer.abort();
        }
        self.clear_git_watchers();
        *lock(&self.state.cached_branch) = None;
        *lock(&self.state.git_paths) = find_git_paths(&cwd);
        self.setup_git_watcher();
        self.state.notify_branch_change();
    }

    pub fn dispose(&self) {
        self.state.disposed.store(true, Ordering::SeqCst);
        if let Some(timer) = lock(&self.state.refresh_timer).take() {
            timer.abort();
        }
        self.clear_git_watchers();
        lock(&self.state.branch_change_callbacks).clear();
    }

    fn schedule_refresh(&self) {
        let mut timer = lock(&self.state.refresh_timer);
        if self.state.disposed.load(Ordering::SeqCst) || timer.is_some() {
            return;
        }
        if self.state.refresh_in_flight.load(Ordering::SeqCst) {
            self.state.refresh_pending.store(true, Ordering::SeqCst);
            return;
        }
        let provider = self.clone();
        let handle = self.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(WATCH_DEBOUNCE_MS)).await;
            *lock(&provider.state.refresh_timer) = None;
            provider.refresh_git_branch_async().await;
        });
        *timer = Some(handle);
    }

    async fn refresh_git_branch_async(&self) {
        if self.state.disposed.load(Ordering::SeqCst) {
            return;
        }
        if self.state.refresh_in_flight.swap(true, Ordering::SeqCst) {
            self.state.refresh_pending.store(true, Ordering::SeqCst);
            return;
        }
        let next_branch = self.state.resolve_branch_async().await;
        if self.state.disposed.load(Ordering::SeqCst) {
            self.state.refresh_in_flight.store(false, Ordering::SeqCst);
            return;
        }
        let previous = lock(&self.state.cached_branch).clone();
        *lock(&self.state.cached_branch) = Some(next_branch.clone());
        self.state.refresh_in_flight.store(false, Ordering::SeqCst);
        if let Some(previous) = previous
            && previous != next_branch
        {
            self.state.notify_branch_change();
        }
        if self.state.refresh_pending.swap(false, Ordering::SeqCst) && !self.state.disposed.load(Ordering::SeqCst) {
            self.schedule_refresh();
        }
    }

    fn clear_git_watchers(&self) {
        close_watcher(lock(&self.state.head_watcher).take());
        close_watcher(lock(&self.state.reftable_watcher).take());
        close_watcher(lock(&self.state.reftable_tables_list_watcher).take());
        if let Some(timer) = lock(&self.state.poll_timer).take() {
            timer.abort();
        }
        if let Some(timer) = lock(&self.state.git_watcher_retry_timer).take() {
            timer.abort();
        }
    }

    fn schedule_git_watcher_retry(&self) {
        if self.state.disposed.load(Ordering::SeqCst) || lock(&self.state.git_watcher_retry_timer).is_some() {
            return;
        }
        let provider = self.clone();
        let handle = self.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_millis(FS_WATCH_RETRY_DELAY_MS)).await;
            *lock(&provider.state.git_watcher_retry_timer) = None;
            provider.setup_git_watcher();
        });
        *lock(&self.state.git_watcher_retry_timer) = Some(handle);
    }

    fn handle_git_watcher_error(&self) {
        self.clear_git_watchers();
        self.schedule_git_watcher_retry();
    }

    fn setup_git_watcher(&self) {
        self.clear_git_watchers();
        let Some(paths) = lock(&self.state.git_paths).clone() else {
            return;
        };
        let poll_git_head = should_poll_git_head(&paths.repo_dir);
        let head_dir = node_dirname(&paths.head_path);

        let head_provider = self.clone();
        let head_watcher = watch_with_error_handler(
            Path::new(&encode_node_utf8(&head_dir)),
            move |_kind: WatchEventKind, filename: Option<PathBuf>| {
                let matches = filename
                    .as_ref()
                    .is_none_or(|path| path.file_name().is_some_and(|name| name == "HEAD"));
                if matches {
                    head_provider.schedule_refresh();
                }
            },
            {
                let provider = self.clone();
                move || provider.handle_git_watcher_error()
            },
        );
        *lock(&self.state.head_watcher) = head_watcher;

        let reftable_dir = node_join(&paths.common_git_dir, "reftable");
        if path_exists(&reftable_dir) {
            let reftable_provider = self.clone();
            let reftable_watcher = watch_with_error_handler(
                Path::new(&encode_node_utf8(&reftable_dir)),
                move |_kind: WatchEventKind, _filename: Option<PathBuf>| {
                    reftable_provider.schedule_refresh();
                },
                {
                    let provider = self.clone();
                    move || provider.handle_git_watcher_error()
                },
            );
            *lock(&self.state.reftable_watcher) = reftable_watcher;

            let tables_list = node_join(&reftable_dir, "tables.list");
            if path_exists(&tables_list) {
                let list_provider = self.clone();
                let list_watcher = watch_with_error_handler(
                    Path::new(&encode_node_utf8(&tables_list)),
                    move |_kind: WatchEventKind, _filename: Option<PathBuf>| {
                        list_provider.schedule_refresh();
                    },
                    {
                        let provider = self.clone();
                        move || provider.handle_git_watcher_error()
                    },
                );
                *lock(&self.state.reftable_tables_list_watcher) = list_watcher;
            }
        }

        if lock(&self.state.head_watcher).is_none() && !poll_git_head {
            return;
        }

        let provider = self.clone();
        let poll_tables_list = path_exists(&node_join(&reftable_dir, "tables.list"));
        let poll_head = poll_git_head;
        let head_path = paths.head_path.clone();
        let handle = self.runtime.spawn(async move {
            let mut head_stamp = file_stamp(&head_path);
            let mut tables_stamp = file_stamp(&node_join(&reftable_dir, "tables.list"));
            let mut ticker = tokio::time::interval(Duration::from_millis(GIT_FILE_POLL_INTERVAL_MS));
            loop {
                ticker.tick().await;
                if provider.state.disposed.load(Ordering::SeqCst) {
                    return;
                }
                if poll_head {
                    let current = file_stamp(&head_path);
                    if current != head_stamp {
                        head_stamp = current;
                        provider.schedule_refresh();
                    }
                }
                if poll_tables_list {
                    let path = provider
                        .state
                        .git_paths
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .as_ref()
                        .map(|paths| node_join(&paths.common_git_dir, "reftable/tables.list"));
                    let Some(path) = path else {
                        continue;
                    };
                    let current = file_stamp(&path);
                    if current != tables_stamp {
                        tables_stamp = current;
                        provider.schedule_refresh();
                    }
                }
            }
        });
        *lock(&self.state.poll_timer) = Some(handle);
    }
}

fn file_stamp(path: &JsString) -> Option<(u128, u128, u64)> {
    let metadata = std::fs::metadata(encode_node_utf8(path)).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let created = metadata
        .created()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    Some((modified, created, metadata.len()))
}

/// Unsubscribes a branch-change callback on drop.
pub struct BranchChangeSubscription {
    state: Arc<FooterState>,
    id: usize,
}

impl Drop for BranchChangeSubscription {
    fn drop(&mut self) {
        self.state.unsubscribe(self.id);
    }
}
