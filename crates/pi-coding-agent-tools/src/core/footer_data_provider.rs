//! Port of `packages/coding-agent/src/core/footer-data-provider.ts`.
//!
//! Footer state (git branch, extension statuses, provider count) is data the
//! extension API cannot otherwise reach. Branch detection reads `.git/HEAD`
//! directly and falls back to `git symbolic-ref` for reftable repos, whose HEAD
//! contains the `.invalid` placeholder.

use crate::utils::paths::{node_basename, node_dirname, node_join, node_resolve};
use indexmap::IndexMap;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_ai::request::{BoxFuture, CallbackError};
use pi_ai::types::Model;
use tokio::task::JoinHandle;

use crate::core::readonly_session_manager::ReadonlySessionManager;
use crate::utils::fs_watch::{FS_WATCH_RETRY_DELAY_MS, WatchEventKind, watch_with_error_handler};

#[doc(hidden)]
pub type FooterWatchListener = Box<dyn Fn(WatchEventKind, Option<PathBuf>) + Send>;
#[doc(hidden)]
pub type FooterWatchFailure = Arc<dyn Fn() + Send + Sync>;
#[doc(hidden)]
pub type FooterWatchHandle = Box<dyn Send>;
#[doc(hidden)]
pub type FooterWatchFactory =
    Arc<dyn Fn(&Path, FooterWatchListener, FooterWatchFailure) -> Option<FooterWatchHandle> + Send + Sync>;

fn watch_system_path(
    path: &Path,
    listener: FooterWatchListener,
    failed: FooterWatchFailure,
) -> Option<FooterWatchHandle> {
    watch_with_error_handler(path, listener, move || failed()).map(|watcher| Box::new(watcher) as FooterWatchHandle)
}

/// Debounce for file-system triggered branch refreshes.
const WATCH_DEBOUNCE_MS: u64 = 500;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_refresh;
#[cfg(test)]
mod tests_runtime_watch;
/// Poll interval for WSL Windows-mounted `HEAD` files only.
const GIT_FILE_POLL_INTERVAL_MS: u64 = 1_000;

type BranchChangeCallback = (usize, Arc<dyn Fn() -> Result<(), CallbackError> + Send + Sync>);

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
    pub session_manager: Option<ReadonlySessionManager>,
    pub model: Option<Model>,
    pub thinking_level: JsString,
    pub context_usage: Option<ContextUsage>,
}

pub type ExtensionStatusCallback =
    Arc<dyn Fn(Option<FooterSessionOverride>) -> Result<Option<JsString>, CallbackError> + Send + Sync>;

/// Frozen footer text or a callback evaluated when the status is read.
#[derive(Clone)]
pub enum ExtensionStatus {
    Text(JsString),
    Callback(ExtensionStatusCallback),
}

impl ExtensionStatus {
    pub fn evaluate(&self, view: Option<FooterSessionOverride>) -> Result<Option<JsString>, CallbackError> {
        match self {
            Self::Text(text) => Ok(Some(text.clone())),
            Self::Callback(callback) => callback(view),
        }
    }
}

/// Runs `git symbolic-ref` for reftable repos. Injectable so tests can decide
/// what the `.invalid` placeholder resolves to.
pub trait GitBranchRunner: Send + Sync {
    fn resolve_sync(&self, repo_dir: &JsString) -> Option<JsString>;
    fn resolve_async(&self, repo_dir: &JsString) -> BoxFuture<'static, Option<JsString>>;
}

/// Spawns the real `git` executable.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemGitBranchRunner;

const GIT_BRANCH_ARGS: [&str; 5] = ["--no-optional-locks", "symbolic-ref", "--quiet", "--short", "HEAD"];
const GIT_MAX_BUFFER_BYTES: usize = 1024 * 1024;

fn decode_git_branch(bytes: &[u8]) -> Option<JsString> {
    let branch = JsString::from(String::from_utf8_lossy(bytes).as_ref()).trim();
    (!branch.is_empty()).then_some(branch)
}

impl GitBranchRunner for SystemGitBranchRunner {
    fn resolve_sync(&self, repo_dir: &JsString) -> Option<JsString> {
        use crate::utils::child_process::{ChildStdio, ChildStdioOptions, SpawnSyncOptions, spawn_sync};
        let cwd = encode_node_utf8(repo_dir);
        let arguments = GIT_BRANCH_ARGS.map(JsString::from);
        let output = spawn_sync(
            &"git".into(),
            &arguments,
            SpawnSyncOptions {
                cwd: Some(Path::new(&cwd)),
                max_buffer: Some(GIT_MAX_BUFFER_BYTES),
                stdio: ChildStdioOptions {
                    stdin: ChildStdio::Ignore,
                    stdout: ChildStdio::Pipe,
                    stderr: ChildStdio::Ignore,
                },
                ..Default::default()
            },
        )
        .ok()?;
        if output.status != Some(0) {
            return None;
        }
        decode_git_branch(output.stdout.as_deref().expect("successful piped git stdout"))
    }

    fn resolve_async(&self, repo_dir: &JsString) -> BoxFuture<'static, Option<JsString>> {
        let repo_dir = repo_dir.clone();
        Box::pin(async move {
            use std::process::Stdio;
            let mut child = tokio::process::Command::new("git")
                .args(GIT_BRANCH_ARGS)
                .current_dir(encode_node_utf8(&repo_dir))
                .env_clear()
                .envs(pi_js_compat::process_env::vars())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .ok()?;
            let captured = crate::utils::child_process::capture_child_process(
                &mut child,
                Some(GIT_MAX_BUFFER_BYTES),
                None,
                rustix::process::Signal::TERM,
            )
            .await
            .ok()?;
            if captured.stopped.is_some() || captured.output.status != Some(0) {
                return None;
            }
            decode_git_branch(&captured.output.stdout)
        })
    }
}

/// Finds git metadata by walking up from `cwd`.
pub fn find_git_paths(cwd: &JsString) -> Option<GitPaths> {
    let mut dir = cwd.clone();
    loop {
        let git_path = node_join(&dir, ".git");
        if Path::new(&encode_node_utf8(&git_path)).exists() {
            match std::fs::metadata(encode_node_utf8(&git_path)) {
                Ok(metadata) if metadata.is_file() => {
                    let content = read_git_text(&git_path)?.trim();
                    if let Some(rest) = content.strip_prefix("gitdir: ") {
                        return read_worktree_git_paths(&dir, &rest);
                    }
                }
                Ok(metadata) if metadata.is_dir() => {
                    let head_path = node_join(&git_path, "HEAD");
                    if !Path::new(&encode_node_utf8(&head_path)).exists() {
                        return None;
                    }
                    return Some(GitPaths {
                        repo_dir: dir,
                        common_git_dir: git_path,
                        head_path,
                    });
                }
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        let parent = node_dirname(&dir);
        if parent == dir {
            return None;
        }
        dir = parent;
    }
}

fn read_worktree_git_paths(dir: &JsString, git_dir_text: &JsString) -> Option<GitPaths> {
    let git_dir = node_resolve(dir, git_dir_text.trim()).ok()?;
    let head_path = node_join(&git_dir, "HEAD");
    if !Path::new(&encode_node_utf8(&head_path)).exists() {
        return None;
    }
    let common_dir_path = node_join(&git_dir, "commondir");
    let common_git_dir = if Path::new(&encode_node_utf8(&common_dir_path)).exists() {
        node_resolve(&git_dir, read_git_text(&common_dir_path)?.trim()).ok()?
    } else {
        git_dir
    };
    Some(GitPaths {
        repo_dir: dir.clone(),
        common_git_dir,
        head_path,
    })
}

fn read_git_text(path: &JsString) -> Option<JsString> {
    let bytes = std::fs::read(encode_node_utf8(path)).ok()?;
    Some(JsString::from(String::from_utf8_lossy(&bytes).as_ref()))
}

/// `normalizeFooterExecutableName`: `None` for the default `pi` binary.
pub fn normalize_footer_executable_name(name: &JsString) -> Option<JsString> {
    if name.is_empty() || name == "pi" {
        return None;
    }
    name.starts_with("pi-").then(|| name.clone())
}

/// Resolves the executable name shown by the footer.
pub fn resolve_footer_executable_name_with_env(
    paths: &[JsString],
    configured_name: Option<&JsString>,
) -> Option<JsString> {
    if let Some(name) = configured_name.and_then(normalize_footer_executable_name) {
        return Some(name);
    }
    paths
        .iter()
        .filter(|path| !path.is_empty())
        .map(node_basename)
        .find_map(|name| normalize_footer_executable_name(&name))
}

/// Resolves the executable name from `PI_EXECUTABLE_NAME` and process argv/execPath.
pub fn resolve_footer_executable_name() -> Option<JsString> {
    let configured = pi_js_compat::process_env::get("PI_EXECUTABLE_NAME").map(JsString::from);
    let argv1 = std::env::args_os()
        .nth(1)
        .map(|value| JsString::from(value.to_string_lossy().as_ref()));
    let exec_path = std::env::current_exe()
        .ok()
        .map(|value| JsString::from(value.to_string_lossy().as_ref()));
    let paths = argv1.into_iter().chain(exec_path).collect::<Vec<_>>();
    resolve_footer_executable_name_with_env(&paths, configured.as_ref())
}

fn is_wsl_environment() -> bool {
    cfg!(target_os = "linux")
        && (pi_js_compat::process_env::get("WSL_DISTRO_NAME").is_some_and(|value| !value.is_empty())
            || pi_js_compat::process_env::get("WSL_INTEROP").is_some_and(|value| !value.is_empty()))
}

fn is_windows_mounted_repo_path(path: &JsString) -> bool {
    let units = path.units();
    let prefix_matches = units.get(..5).is_some_and(|prefix| {
        prefix
            .iter()
            .copied()
            .map(|unit| match unit {
                65..=90 => unit + 32,
                _ => unit,
            })
            .eq("/mnt/".encode_utf16())
    });
    let drive_matches = units.get(5).is_some_and(|unit| matches!(unit, 65..=90 | 97..=122));
    prefix_matches && drive_matches && matches!(units.get(6), None | Some(47))
}

fn should_poll_git_head(repo_dir: &JsString) -> bool {
    is_wsl_environment() && is_windows_mounted_repo_path(repo_dir)
}

struct FooterState {
    cwd: Mutex<JsString>,
    extension_statuses: Mutex<IndexMap<JsString, ExtensionStatus>>,
    /// `None` = not computed yet, `Some(None)` = not a repo / detached.
    cached_branch: Mutex<Option<Option<JsString>>>,
    git_paths: Mutex<Option<GitPaths>>,
    branch_change_callbacks: Mutex<Vec<BranchChangeCallback>>,
    next_callback_id: AtomicUsize,
    available_provider_count: AtomicU64,
    refresh_timer: Mutex<Option<JoinHandle<()>>>,
    git_watcher_retry_timer: Mutex<Option<JoinHandle<()>>>,
    refresh_in_flight: AtomicBool,
    refresh_pending: AtomicBool,
    disposed: AtomicBool,
    session_override: Mutex<Option<FooterSessionOverride>>,
    head_watcher: Mutex<Option<FooterWatchHandle>>,
    reftable_watcher: Mutex<Option<FooterWatchHandle>>,
    reftable_tables_list_watcher: Mutex<Option<FooterWatchHandle>>,
    poll_timer: Mutex<Option<JoinHandle<()>>>,
    git: Arc<dyn GitBranchRunner>,
    watch: FooterWatchFactory,
}

impl FooterState {
    fn notify_branch_change(&self) -> Result<(), CallbackError> {
        let mut previous = None;
        loop {
            let next = lock(&self.branch_change_callbacks)
                .iter()
                .find(|(id, _)| previous.is_none_or(|previous| *id > previous))
                .cloned();
            let Some((id, callback)) = next else {
                return Ok(());
            };
            previous = Some(id);
            callback()?;
        }
    }

    fn unsubscribe(&self, id: usize) {
        lock(&self.branch_change_callbacks).retain(|(callback_id, _)| *callback_id != id);
    }

    fn resolve_branch_sync(&self) -> Option<JsString> {
        let paths = lock(&self.git_paths).clone()?;
        let content = read_git_text(&paths.head_path)?;
        let content = content.trim();
        if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
            if branch == ".invalid" {
                return Some(
                    self.git
                        .resolve_sync(&paths.repo_dir)
                        .unwrap_or_else(|| "detached".into()),
                );
            }
            return Some(branch.to_owned());
        }
        Some("detached".into())
    }

    async fn resolve_branch_async(&self) -> Option<JsString> {
        let paths = lock(&self.git_paths).clone()?;
        let content = read_git_text(&paths.head_path)?;
        let content = content.trim();
        if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
            if branch == ".invalid" {
                return Some(
                    self.git
                        .resolve_async(&paths.repo_dir)
                        .await
                        .unwrap_or_else(|| "detached".into()),
                );
            }
            return Some(branch.to_owned());
        }
        Some("detached".into())
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
        Self::with_git_and_watch_factory(cwd, git, Arc::new(watch_system_path))
    }

    /// Footer-only fault injection for runtime watcher failures absent from real inotify probes.
    #[doc(hidden)]
    pub fn with_git_and_watch_factory(
        cwd: impl Into<JsString>,
        git: Arc<dyn GitBranchRunner>,
        watch: FooterWatchFactory,
    ) -> Self {
        let cwd = cwd.into();
        let state = Arc::new(FooterState {
            git_paths: Mutex::new(find_git_paths(&cwd)),
            cwd: Mutex::new(cwd),
            extension_statuses: Mutex::new(IndexMap::new()),
            cached_branch: Mutex::new(None),
            branch_change_callbacks: Mutex::new(Vec::new()),
            next_callback_id: AtomicUsize::new(0),
            available_provider_count: AtomicU64::new(0.0_f64.to_bits()),
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
            watch,
        });
        let provider = Self {
            state,
            runtime: tokio::runtime::Handle::current(),
        };
        provider.setup_git_watcher();
        provider
    }

    /// Current branch; `None` when not in a repository, `"detached"` for detached HEAD.
    pub fn get_git_branch(&self) -> Option<JsString> {
        let mut cached = lock(&self.state.cached_branch);
        if cached.is_none() {
            *cached = Some(self.state.resolve_branch_sync());
        }
        cached.clone().flatten()
    }

    pub fn get_executable_name(&self) -> Option<JsString> {
        resolve_footer_executable_name()
    }

    pub fn get_extension_statuses(&self) -> Result<IndexMap<JsString, JsString>, CallbackError> {
        let statuses = lock(&self.state.extension_statuses).clone();
        let mut texts = IndexMap::new();
        for (key, status) in statuses {
            if let Some(text) = status.evaluate(self.get_session_override())? {
                texts.insert(key, text);
            }
        }
        Ok(texts)
    }

    /// Subscribes to branch changes; dropping the subscription unsubscribes.
    pub fn on_branch_change(
        &self,
        callback: impl Fn() -> Result<(), CallbackError> + Send + Sync + 'static,
    ) -> BranchChangeSubscription {
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

    pub fn set_extension_status(&self, key: &JsString, text: Option<ExtensionStatus>) {
        let mut statuses = lock(&self.state.extension_statuses);
        match text {
            Some(text) => {
                statuses.insert(key.to_owned(), text);
            }
            None => {
                statuses.shift_remove(key);
            }
        }
    }

    pub fn clear_extension_statuses(&self) {
        lock(&self.state.extension_statuses).clear();
    }

    pub fn get_available_provider_count(&self) -> f64 {
        f64::from_bits(self.state.available_provider_count.load(Ordering::SeqCst))
    }

    pub fn set_available_provider_count(&self, count: f64) {
        self.state
            .available_provider_count
            .store(count.to_bits(), Ordering::SeqCst);
    }

    pub fn set_cwd(&self, cwd: impl Into<JsString>) -> Result<(), CallbackError> {
        let cwd = cwd.into();
        {
            let mut current = lock(&self.state.cwd);
            if *current == cwd {
                return Ok(());
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
        self.state.notify_branch_change()
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
            provider
                .refresh_git_branch_async()
                .await
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        });
        *timer = Some(handle);
    }

    async fn refresh_git_branch_async(&self) -> Result<(), CallbackError> {
        if self.state.disposed.load(Ordering::SeqCst) {
            return Ok(());
        }
        if self.state.refresh_in_flight.swap(true, Ordering::SeqCst) {
            self.state.refresh_pending.store(true, Ordering::SeqCst);
            return Ok(());
        }
        let next_branch = self.state.resolve_branch_async().await;
        if self.state.disposed.load(Ordering::SeqCst) {
            self.state.refresh_in_flight.store(false, Ordering::SeqCst);
            return Ok(());
        }
        let previous = lock(&self.state.cached_branch).replace(next_branch.clone());
        self.state.refresh_in_flight.store(false, Ordering::SeqCst);
        let result = if previous.is_some_and(|previous| previous != next_branch) {
            self.state.notify_branch_change()
        } else {
            Ok(())
        };
        if self.state.refresh_pending.swap(false, Ordering::SeqCst) && !self.state.disposed.load(Ordering::SeqCst) {
            self.schedule_refresh();
        }
        result
    }

    fn clear_git_watchers(&self) {
        drop(lock(&self.state.head_watcher).take());
        drop(lock(&self.state.reftable_watcher).take());
        drop(lock(&self.state.reftable_tables_list_watcher).take());
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
        let head_watcher = (self.state.watch)(
            Path::new(&encode_node_utf8(&head_dir)),
            Box::new(move |_kind: WatchEventKind, filename: Option<PathBuf>| {
                let matches = filename
                    .as_ref()
                    .is_none_or(|path| path.file_name().is_some_and(|name| name == "HEAD"));
                if matches {
                    head_provider.schedule_refresh();
                }
            }),
            {
                let provider = self.clone();
                Arc::new(move || provider.handle_git_watcher_error())
            },
        );
        *lock(&self.state.head_watcher) = head_watcher;
        if lock(&self.state.head_watcher).is_none() && !poll_git_head {
            return;
        }

        let reftable_dir = node_join(&paths.common_git_dir, "reftable");
        if Path::new(&encode_node_utf8(&reftable_dir)).exists() {
            let reftable_provider = self.clone();
            let reftable_watcher = (self.state.watch)(
                Path::new(&encode_node_utf8(&reftable_dir)),
                Box::new(move |_kind: WatchEventKind, _filename: Option<PathBuf>| {
                    reftable_provider.schedule_refresh();
                }),
                {
                    let provider = self.clone();
                    Arc::new(move || provider.handle_git_watcher_error())
                },
            );
            *lock(&self.state.reftable_watcher) = reftable_watcher;
            if lock(&self.state.reftable_watcher).is_none() {
                return;
            }

            let tables_list = node_join(&reftable_dir, "tables.list");
            if Path::new(&encode_node_utf8(&tables_list)).exists() {
                let list_provider = self.clone();
                let list_watcher = (self.state.watch)(
                    Path::new(&encode_node_utf8(&tables_list)),
                    Box::new(move |_kind: WatchEventKind, _filename: Option<PathBuf>| {
                        list_provider.schedule_refresh();
                    }),
                    {
                        let provider = self.clone();
                        Arc::new(move || provider.handle_git_watcher_error())
                    },
                );
                *lock(&self.state.reftable_tables_list_watcher) = list_watcher;
            }
        }

        if !poll_git_head {
            return;
        }
        let provider = self.clone();
        let head_path = paths.head_path.clone();
        let handle = self.runtime.spawn(async move {
            let mut head_stamp = file_stamp(&head_path);
            let mut ticker = tokio::time::interval(Duration::from_millis(GIT_FILE_POLL_INTERVAL_MS));
            loop {
                ticker.tick().await;
                if provider.state.disposed.load(Ordering::SeqCst) {
                    return;
                }
                let current = file_stamp(&head_path);
                if current != head_stamp {
                    head_stamp = current;
                    provider.schedule_refresh();
                }
            }
        });
        *lock(&self.state.poll_timer) = Some(handle);
    }
}

fn file_stamp(path: &JsString) -> Option<(f64, f64, f64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(encode_node_utf8(path)).ok()?;
    let modified_ms = metadata.mtime() as f64 * 1000.0 + metadata.mtime_nsec() as f64 / 1_000_000.0;
    let changed_ms = metadata.ctime() as f64 * 1000.0 + metadata.ctime_nsec() as f64 / 1_000_000.0;
    Some((modified_ms, changed_ms, metadata.len() as f64))
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
