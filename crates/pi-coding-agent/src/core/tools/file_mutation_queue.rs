//! Serializes file mutations that target the same canonical path.

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use crate::core::extensions::types::HandlerError;
use crate::utils::paths::node_resolve;
use pi_js_compat::js_json::{JsString, JsValue};
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
async fn mutation_queue_key(file_path: &JsString) -> io::Result<JsString> {
    let resolved = node_resolve("", file_path).map_err(|error| io::Error::other(HandlerError::from(error)))?;
    match pi_js_compat::node_fs::realpath(&resolved).await {
        Ok(canonical) => Ok(canonical),
        Err(error)
            if error
                .value()
                .get("code")
                .and_then(JsValue::as_string)
                .is_some_and(|code| code == "ENOENT" || code == "ENOTDIR") =>
        {
            Ok(resolved)
        }
        Err(error) => Err(io::Error::other(HandlerError::from(error))),
    }
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
    let generation = next_generation();
    let (release, released) = oneshot::channel();
    let (key, previous) = {
        let _registration = registration().await;
        let key = mutation_queue_key(file_path).await?;
        let previous = queues().insert(key.clone(), Tail { generation, released });
        (key, previous)
    };
    if let Some(previous) = previous {
        let _ = previous.released.await;
    }

    let guard = QueueGuard { key, generation };
    let result = operation().await;
    let _ = release.send(());
    drop(guard);
    Ok(result)
}

/// Serializes async key resolution and queue registration in source call order.
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
