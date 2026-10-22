//! Serializes file mutations that target the same canonical path.

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use crate::utils::paths::node_resolve;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use tokio::sync::oneshot;

struct Tail {
    generation: u64,
    released: oneshot::Receiver<()>,
}

fn queues() -> MutexGuard<'static, HashMap<JsString, Tail>> {
    static QUEUES: LazyLock<Mutex<HashMap<JsString, Tail>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
    QUEUES.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Resolves `file_path` to an absolute path and, when it exists, its canonical form.
fn mutation_queue_key(file_path: &JsString) -> io::Result<JsString> {
    let resolved = node_resolve("", file_path).map_err(|error| io::Error::other(error.to_string()))?;
    match std::fs::canonicalize(encode_node_utf8(&resolved)) {
        Ok(canonical) => Ok(canonical.to_string_lossy().into_owned().into()),
        Err(error) if is_missing_path_error(&error) => Ok(resolved),
        Err(error) => Err(error),
    }
}

fn is_missing_path_error(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory)
}

/// Runs `operation` after every earlier mutation of the same file has finished.
///
/// Operations for different files still run in parallel. The queue is released
/// even when the operation fails or panics. Fails only when the path cannot be
/// resolved; the operation's own result is returned unchanged.
pub async fn with_file_mutation_queue<T, F, Fut>(file_path: &JsString, operation: F) -> io::Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let key = mutation_queue_key(file_path)?;
    Ok(run_queued(key, operation).await)
}

async fn run_queued<T, F, Fut>(key: JsString, operation: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let generation = next_generation();
    let (release, released) = oneshot::channel();
    let previous = {
        let _registration = registration().await;
        queues().insert(key.clone(), Tail { generation, released })
    };
    if let Some(previous) = previous {
        let _ = previous.released.await;
    }

    let guard = QueueGuard { key, generation };
    let result = operation().await;
    let _ = release.send(());
    drop(guard);
    result
}

/// Serializes the read-modify-write of the queue map so a new registration never
/// misses the operation it must wait for. Held only across synchronous map access.
async fn registration() -> tokio::sync::MutexGuard<'static, ()> {
    static REGISTRATION: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));
    REGISTRATION.lock().await
}

fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

struct QueueGuard {
    key: JsString,
    generation: u64,
}

impl Drop for QueueGuard {
    fn drop(&mut self) {
        let mut queues = queues();
        let is_current = queues
            .get(&self.key)
            .is_some_and(|tail| tail.generation == self.generation);
        if is_current {
            queues.remove(&self.key);
        }
    }
}
