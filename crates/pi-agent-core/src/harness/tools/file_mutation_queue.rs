use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::harness::types::{ExecutionEnv, FileError, FileErrorCode};

/// Serializes file mutations targeting the same environment and canonical path.
#[derive(Debug, Default)]
pub struct FileMutationQueue {
    state: Mutex<QueueState>,
}

#[derive(Debug, Default)]
struct QueueState {
    entries: HashMap<String, QueueEntry>,
}

#[derive(Debug)]
struct QueueEntry {
    lock: Arc<Mutex<()>>,
    users: usize,
}

impl FileMutationQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `operation` while holding the mutation lock for `path`.
    pub async fn run<T, F>(&self, env: &dyn ExecutionEnv, path: &str, operation: F) -> Result<T, FileError>
    where
        T: Send,
        F: Future<Output = T> + Send,
    {
        let key = mutation_queue_key(env, path).await?;
        let lock = self.register(&key).await;
        let guard = lock.lock().await;
        let value = operation.await;
        drop(guard);
        self.release(&key, &lock).await;
        Ok(value)
    }

    async fn register(&self, key: &str) -> Arc<Mutex<()>> {
        let mut state = self.state.lock().await;
        let entry = state.entries.entry(key.to_owned()).or_insert_with(|| QueueEntry {
            lock: Arc::new(Mutex::new(())),
            users: 0,
        });
        entry.users += 1;
        Arc::clone(&entry.lock)
    }

    async fn release(&self, key: &str, lock: &Arc<Mutex<()>>) {
        let mut state = self.state.lock().await;
        let Some(entry) = state.entries.get_mut(key) else {
            return;
        };
        if !Arc::ptr_eq(&entry.lock, lock) {
            return;
        }
        entry.users -= 1;
        if entry.users == 0 {
            state.entries.remove(key);
        }
    }
}

async fn mutation_queue_key(env: &dyn ExecutionEnv, path: &str) -> Result<String, FileError> {
    let absolute_path = env.absolute_path(path, None).await?;
    match env.canonical_path(&absolute_path).await {
        Ok(canonical_path) => Ok(canonical_path),
        Err(error) if matches!(error.code, FileErrorCode::NotFound | FileErrorCode::NotSupported) => Ok(absolute_path),
        Err(error) => Err(error),
    }
}
