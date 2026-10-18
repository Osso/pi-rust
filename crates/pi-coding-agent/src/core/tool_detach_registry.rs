//! Auto-detaching tool registry (`core/tool-detach-registry.ts`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

pub const DEFAULT_AUTO_DETACH_AFTER_MS: u64 = 600_000;

pub trait ToolDetachHandle: Send + Sync {
    fn detach(&self) -> bool;
}

struct RegisteredHandle {
    id: u64,
    handle: Arc<dyn ToolDetachHandle>,
}

type SharedHandles = Arc<Mutex<Vec<RegisteredHandle>>>;
type WeakHandles = Weak<Mutex<Vec<RegisteredHandle>>>;

pub struct ToolDetachRegistry {
    auto_detach_after_ms: u64,
    handles: SharedHandles,
    next_id: AtomicU64,
}

impl ToolDetachRegistry {
    pub fn new() -> Self {
        Self::with_auto_detach_after_ms(DEFAULT_AUTO_DETACH_AFTER_MS)
    }

    pub fn with_auto_detach_after_ms(auto_detach_after_ms: u64) -> Self {
        Self {
            auto_detach_after_ms,
            handles: Arc::new(Mutex::new(Vec::new())),
            next_id: AtomicU64::new(1),
        }
    }

    /// Register a handle; dropping the returned subscription cancels its timer.
    pub fn register(self: &Arc<Self>, handle: Arc<dyn ToolDetachHandle>) -> DetachSubscription {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.handles().push(RegisteredHandle {
            id,
            handle: Arc::clone(&handle),
        });
        let (cancel, cancelled) = std::sync::mpsc::channel();
        spawn_auto_detach_timer(
            self.auto_detach_after_ms,
            id,
            handle,
            Arc::downgrade(&self.handles),
            cancelled,
        );
        DetachSubscription {
            registry: Arc::downgrade(self),
            id,
            cancel: Mutex::new(Some(cancel)),
        }
    }

    /// Detach the most recently registered handle that accepts detachment.
    pub fn detach_running(&self) -> bool {
        let handles: Vec<Arc<dyn ToolDetachHandle>> = self
            .handles()
            .iter()
            .rev()
            .map(|registered| Arc::clone(&registered.handle))
            .collect();
        handles.into_iter().any(|handle| handle.detach())
    }

    pub fn has_running(&self) -> bool {
        !self.handles().is_empty()
    }

    fn handles(&self) -> MutexGuard<'_, Vec<RegisteredHandle>> {
        self.handles.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn unregister(&self, id: u64) {
        self.handles().retain(|registered| registered.id != id);
    }
}

impl Default for ToolDetachRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// `setTimeout(...).unref()` equivalent: a detached thread that never blocks exit,
/// cancelled by the subscription's channel send.
fn spawn_auto_detach_timer(
    auto_detach_after_ms: u64,
    id: u64,
    handle: Arc<dyn ToolDetachHandle>,
    handles: WeakHandles,
    cancelled: std::sync::mpsc::Receiver<()>,
) {
    std::thread::spawn(move || {
        if cancelled.recv_timeout(Duration::from_millis(auto_detach_after_ms)) != Err(RecvTimeoutError::Timeout) {
            return;
        }
        let Some(handles) = handles.upgrade() else {
            return;
        };
        let still_registered = handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .any(|registered| registered.id == id);
        if still_registered {
            handle.detach();
        }
    });
}

pub struct DetachSubscription {
    registry: Weak<ToolDetachRegistry>,
    id: u64,
    cancel: Mutex<Option<Sender<()>>>,
}

impl DetachSubscription {
    pub fn unregister(self) {
        self.unregister_ref();
    }

    fn unregister_ref(&self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.unregister(self.id);
        }
        if let Some(cancel) = self
            .cancel
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = cancel.send(());
        }
    }
}

impl Drop for DetachSubscription {
    fn drop(&mut self) {
        self.unregister_ref();
    }
}
