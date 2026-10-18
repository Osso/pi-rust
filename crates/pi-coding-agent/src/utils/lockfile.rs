//! Port of the `proper-lockfile` protocol used by the coding agent's JSON stores.
//!
//! The TypeScript runtime locks with `proper-lockfile` 4.1.2: the lock is a directory named
//! `<file>.lock`, its mtime marks liveness (set to `ceil(now / 1s) + 5ms` on acquisition and
//! refreshed every `stale / 2`), and a lock whose mtime is older than `stale` is removed and
//! re-acquired. Both runtimes must honor the same protocol so they can share an agent directory.

use std::fs::{self, FileTimes};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

/// `proper-lockfile`'s default `stale` option.
pub const DEFAULT_STALE_MS: u64 = 10_000;
/// `proper-lockfile` clamps `stale` to at least two seconds.
const MIN_STALE_MS: u64 = 2_000;
/// `proper-lockfile` clamps the refresh interval to at most `stale / 2` and at least one second.
const MIN_UPDATE_MS: u64 = 1_000;
/// `retry`'s default `minTimeout` used between acquisition attempts.
const RETRY_MIN_TIMEOUT_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockErrorCode {
    /// `ELOCKED`: another owner holds the lock.
    Locked,
    /// `ECOMPROMISED`: the lock could not be refreshed.
    Compromised,
    /// `ENOTACQUIRED`: release without ownership.
    NotAcquired,
    /// `ERELEASED`: release of an already released or compromised lock.
    Released,
    /// `ESYNC`: retries requested through the synchronous API.
    Sync,
    /// Filesystem failure.
    Io,
}

/// Failure while acquiring, refreshing, or releasing a lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockError {
    pub code: LockErrorCode,
    pub message: String,
    /// The lock path, reported like `proper-lockfile`'s `error.file`.
    pub file: Option<PathBuf>,
}

impl LockError {
    fn new(code: LockErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            file: None,
        }
    }

    fn for_file(mut self, path: &Path) -> Self {
        self.file = Some(path.to_owned());
        self
    }
}

impl std::fmt::Display for LockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LockError {}

type CompromisedCallback = Arc<dyn Fn(LockError) + Send + Sync>;

/// `proper-lockfile` lock options.
#[derive(Clone, Default)]
pub struct LockOptions {
    /// Milliseconds after which an unrefreshed lock is stale. Defaults to ten seconds, clamped to at least two.
    pub stale_ms: u64,
    /// Refresh interval; defaults to `stale / 2`, clamped to `[1s, stale / 2]`.
    pub update_ms: Option<u64>,
    /// Additional acquisition attempts; rejected by the synchronous API.
    pub retries: usize,
    /// Overrides the default `<file>.lock` directory.
    pub lockfile_path: Option<PathBuf>,
    /// Resolve the target through `realpath` before locking. Defaults to true.
    pub realpath: bool,
    /// Called when the lock can no longer be refreshed.
    pub on_compromised: Option<CompromisedCallback>,
}

impl std::fmt::Debug for LockOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LockOptions")
            .field("stale_ms", &self.stale_ms)
            .field("update_ms", &self.update_ms)
            .field("retries", &self.retries)
            .field("lockfile_path", &self.lockfile_path)
            .field("realpath", &self.realpath)
            .field("on_compromised", &self.on_compromised.is_some())
            .finish()
    }
}

impl LockOptions {
    fn normalized_stale_ms(&self) -> u64 {
        self.stale_ms.max(MIN_STALE_MS)
    }

    fn normalized_update_ms(&self) -> u64 {
        let stale = self.normalized_stale_ms();
        self.update_ms.unwrap_or(stale / 2).min(stale / 2).max(MIN_UPDATE_MS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MtimePrecision {
    Seconds,
    Milliseconds,
}

struct LockState {
    lockfile_path: PathBuf,
    mtime: SystemTime,
    precision: MtimePrecision,
    last_update: SystemTime,
    stale: Duration,
    released: bool,
}

/// An acquired lock; releasing or dropping it removes the lock directory.
pub struct LockGuard {
    state: Arc<Mutex<LockState>>,
    refresh: Option<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for LockGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LockGuard")
            .field("lockfile_path", &self.path())
            .field("released", &self.is_released())
            .finish()
    }
}

impl LockGuard {
    /// The lock directory owned by this guard.
    pub fn path(&self) -> PathBuf {
        lock(&self.state).lockfile_path.clone()
    }

    /// Whether the guard was released or compromised.
    pub fn is_released(&self) -> bool {
        lock(&self.state).released
    }

    /// Removes the lock directory; a compromised or released guard reports `ERELEASED`.
    pub fn release(mut self) -> Result<(), LockError> {
        if let Some(refresh) = self.refresh.take() {
            refresh.abort();
        }
        release_state(&self.state)
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Some(refresh) = self.refresh.take() {
            refresh.abort();
        }
        let _ = release_state(&self.state);
    }
}

fn release_state(state: &Arc<Mutex<LockState>>) -> Result<(), LockError> {
    let mut state = lock(state);
    if state.released {
        return Err(LockError::new(LockErrorCode::Released, "Lock is already released"));
    }
    state.released = true;
    let path = state.lockfile_path.clone();
    drop(state);
    match fs::remove_dir(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(&error, "rmdir", &path)),
    }
}

/// Synchronously acquires a lock; `proper-lockfile`'s sync API forbids retries.
pub fn acquire_sync(target: &Path, options: &LockOptions) -> Result<LockGuard, LockError> {
    if options.retries > 0 {
        return Err(LockError::new(
            LockErrorCode::Sync,
            "Cannot use retries with the sync api",
        ));
    }
    let lockfile_path = lockfile_path(target, options)?;
    let state = acquire_until_locked(&lockfile_path, options)?;
    Ok(LockGuard {
        state: Arc::new(Mutex::new(state)),
        refresh: None,
    })
}

/// Acquires a lock, retrying with `retry`'s exponential backoff and refreshing the mtime.
pub async fn acquire(target: &Path, options: &LockOptions) -> Result<LockGuard, LockError> {
    let lockfile_path = lockfile_path(target, options)?;
    let mut attempt = 0;
    let state = loop {
        match acquire_until_locked(&lockfile_path, options) {
            Ok(state) => break state,
            Err(error) if error.code == LockErrorCode::Locked && attempt < options.retries => {
                tokio::time::sleep(retry_delay(attempt)).await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    };

    let state = Arc::new(Mutex::new(state));
    let refresh = spawn_refresh(
        Arc::clone(&state),
        Duration::from_millis(options.normalized_update_ms()),
        options.on_compromised.clone(),
    );
    Ok(LockGuard {
        state,
        refresh: Some(refresh),
    })
}

/// `proper-lockfile`'s `check`: whether a non-stale lock directory exists.
pub fn check(target: &Path, options: &LockOptions) -> Result<bool, LockError> {
    let lockfile_path = lockfile_path(target, options)?;
    match fs::metadata(&lockfile_path) {
        Ok(metadata) => Ok(!is_stale(&metadata, options.normalized_stale_ms())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(&error, "stat", &lockfile_path)),
    }
}

/// `retry`'s exponential backoff for the default options (factor 2, one second minimum).
fn retry_delay(attempt: usize) -> Duration {
    let factor = 2u64.saturating_pow(attempt.min(32) as u32);
    Duration::from_millis(RETRY_MIN_TIMEOUT_MS.saturating_mul(factor))
}

fn lockfile_path(target: &Path, options: &LockOptions) -> Result<PathBuf, LockError> {
    if let Some(path) = &options.lockfile_path {
        return Ok(path.clone());
    }
    let resolved = if options.realpath {
        fs::canonicalize(target).map_err(|error| io_error(&error, "realpath", target))?
    } else {
        target.to_owned()
    };
    let mut name = resolved.into_os_string();
    name.push(".lock");
    Ok(PathBuf::from(name))
}

/// One acquisition attempt; a stale lock directory is removed and re-created.
fn acquire_until_locked(lockfile_path: &Path, options: &LockOptions) -> Result<LockState, LockError> {
    match fs::create_dir(lockfile_path) {
        Ok(()) => return probe_mtime(lockfile_path),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(io_error(&error, "mkdir", lockfile_path)),
    }

    let metadata = match fs::metadata(lockfile_path) {
        Ok(metadata) => metadata,
        // The holder removed the lock meanwhile: retry without another stale check.
        Err(error) if error.kind() == ErrorKind::NotFound => return recreate(lockfile_path),
        Err(error) => return Err(io_error(&error, "stat", lockfile_path)),
    };
    if !is_stale(&metadata, options.normalized_stale_ms()) {
        return Err(locked_error(lockfile_path));
    }
    match fs::remove_dir(lockfile_path) {
        Ok(()) => recreate(lockfile_path),
        Err(error) if error.kind() == ErrorKind::NotFound => recreate(lockfile_path),
        Err(error) => Err(io_error(&error, "rmdir", lockfile_path)),
    }
}

fn recreate(lockfile_path: &Path) -> Result<LockState, LockError> {
    fs::create_dir(lockfile_path).map_err(|error| {
        if error.kind() == ErrorKind::AlreadyExists {
            locked_error(lockfile_path)
        } else {
            io_error(&error, "mkdir", lockfile_path)
        }
    })?;
    probe_mtime(lockfile_path)
}

/// Sets the lock mtime slightly into the future and records the filesystem's mtime precision.
fn probe_mtime(lockfile_path: &Path) -> Result<LockState, LockError> {
    let now_ms = now_millis();
    let probe_ms = ceil_to_second(now_ms) + 5;
    if let Err(error) = set_mtime(lockfile_path, probe_ms) {
        let _ = fs::remove_dir(lockfile_path);
        return Err(error);
    }
    let metadata = fs::metadata(lockfile_path).map_err(|error| {
        let _ = fs::remove_dir(lockfile_path);
        io_error(&error, "stat", lockfile_path)
    })?;
    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let precision = if mtime_millis(&metadata) % 1_000 == 0 {
        MtimePrecision::Seconds
    } else {
        MtimePrecision::Milliseconds
    };
    Ok(LockState {
        lockfile_path: lockfile_path.to_owned(),
        mtime,
        precision,
        last_update: SystemTime::now(),
        stale: Duration::from_millis(DEFAULT_STALE_MS),
        released: false,
    })
}

fn is_stale(metadata: &fs::Metadata, stale_ms: u64) -> bool {
    mtime_millis(metadata) < now_millis() - i64::try_from(stale_ms).unwrap_or(i64::MAX)
}

fn mtime_millis(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// `Math.ceil(millis / 1000) * 1000`.
fn ceil_to_second(millis: i64) -> i64 {
    (millis + 999) / 1_000 * 1_000
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn set_mtime(path: &Path, millis: i64) -> Result<(), LockError> {
    let millis = u64::try_from(millis).unwrap_or(0);
    let timestamp = SystemTime::UNIX_EPOCH + Duration::from_millis(millis);
    let file = fs::File::open(path).map_err(|error| io_error(&error, "open", path))?;
    file.set_times(FileTimes::new().set_modified(timestamp))
        .map_err(|error| io_error(&error, "utimes", path))
}

/// `mtimePrecision.getMtime`: seconds precision rounds up to the next second.
fn refreshed_mtime(precision: MtimePrecision) -> i64 {
    let now = now_millis();
    match precision {
        MtimePrecision::Seconds => ceil_to_second(now),
        MtimePrecision::Milliseconds => now,
    }
}

fn spawn_refresh(
    state: Arc<Mutex<LockState>>,
    interval: Duration,
    on_compromised: Option<CompromisedCallback>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if refresh_once(&state, &on_compromised) {
                return;
            }
        }
    })
}

/// One refresh attempt; returns true once the lock is released or compromised.
fn refresh_once(state: &Arc<Mutex<LockState>>, on_compromised: &Option<CompromisedCallback>) -> bool {
    let (path, expected_mtime, precision, stale_ms, last_update_ms) = {
        let state = lock(state);
        if state.released {
            return true;
        }
        (
            state.lockfile_path.clone(),
            state.mtime,
            state.precision,
            state.stale.as_millis() as i64,
            state
                .last_update
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_millis() as i64),
        )
    };
    let over_threshold = last_update_ms + stale_ms < now_millis();
    match fs::metadata(&path) {
        Ok(metadata) if mtime_millis(&metadata) == system_time_millis(expected_mtime) => {}
        Ok(_) => {
            compromise(
                state,
                on_compromised,
                LockError::new(
                    LockErrorCode::Compromised,
                    "Unable to update lock within the stale threshold",
                )
                .for_file(&path),
            );
            return true;
        }
        Err(error) if error.kind() == ErrorKind::NotFound || over_threshold => {
            compromise(
                state,
                on_compromised,
                LockError::new(
                    LockErrorCode::Compromised,
                    format!("ENOENT: no such file or directory, stat '{}'", path.display()),
                )
                .for_file(&path),
            );
            return true;
        }
        Err(_) => return false,
    }

    let mtime = refreshed_mtime(precision);
    if let Err(error) = set_mtime(&path, mtime) {
        if error.message.contains("ENOENT") || over_threshold {
            compromise(state, on_compromised, error);
            return true;
        }
        return false;
    }
    let mut state = lock(state);
    if state.released {
        return true;
    }
    state.mtime = SystemTime::UNIX_EPOCH + Duration::from_millis(u64::try_from(mtime).unwrap_or(0));
    state.last_update = SystemTime::now();
    false
}

fn compromise(state: &Arc<Mutex<LockState>>, on_compromised: &Option<CompromisedCallback>, error: LockError) {
    {
        let mut state = lock(state);
        if state.released {
            return;
        }
        state.released = true;
    }
    if let Some(callback) = on_compromised {
        callback(error);
    }
}

fn system_time_millis(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn locked_error(lockfile_path: &Path) -> LockError {
    LockError::new(LockErrorCode::Locked, "Lock file is already being held").for_file(lockfile_path)
}

fn io_error(error: &std::io::Error, operation: &str, path: &Path) -> LockError {
    LockError::new(LockErrorCode::Io, format!("{error}: {operation} '{}'", path.display())).for_file(path)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
