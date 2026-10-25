//! Auto-detaching tool registry (`core/tool-detach-registry.ts`).

use indexmap::IndexMap;
use pi_js_compat::callback_error::CallbackError;
use std::collections::HashMap;
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const DEFAULT_AUTO_DETACH_AFTER_MS: u64 = 600_000;

pub trait ToolDetachHandle: Send + Sync {
    fn detach(&self) -> Result<bool, CallbackError>;
}

type Handles = IndexMap<usize, Arc<dyn ToolDetachHandle>>;
type SharedHandles = Arc<Mutex<Handles>>;

pub struct ToolDetachRegistry {
    auto_detach_after_ms: f64,
    handles: SharedHandles,
    timers: Mutex<HashMap<usize, Sender<()>>>,
}

impl ToolDetachRegistry {
    pub fn new() -> Self {
        Self::with_auto_detach_after_ms(DEFAULT_AUTO_DETACH_AFTER_MS as f64)
    }

    pub fn with_auto_detach_after_ms(auto_detach_after_ms: f64) -> Self {
        Self {
            auto_detach_after_ms,
            handles: Arc::new(Mutex::new(IndexMap::new())),
            timers: Mutex::new(HashMap::new()),
        }
    }

    /// Register one handle identity; dropping the subscription unregisters that handle.
    pub fn register(self: &Arc<Self>, handle: Arc<dyn ToolDetachHandle>) -> DetachSubscription {
        let id = Arc::as_ptr(&handle) as *const () as usize;
        self.handles().entry(id).or_insert_with(|| Arc::clone(&handle));
        let (cancel, cancelled) = std::sync::mpsc::channel();
        self.timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, cancel.clone());
        spawn_auto_detach_timer(
            self.auto_detach_after_ms,
            id,
            handle,
            Arc::clone(&self.handles),
            cancelled,
            cancel,
        );
        DetachSubscription {
            registry: Arc::clone(self),
            id,
        }
    }

    /// Detach the newest registered handle that accepts detachment.
    pub fn detach_running(&self) -> Result<bool, CallbackError> {
        let handles: Vec<_> = self.handles().values().rev().cloned().collect();
        // Frozen :26-32 propagates the original callback throw.
        for handle in handles {
            if handle.detach()? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Frozen `detachAll`: snapshot insertion order, propagating the first throw.
    pub fn detach_all(&self) -> Result<(), CallbackError> {
        let handles: Vec<_> = self.handles().values().cloned().collect();
        for handle in handles {
            handle.detach()?;
        }
        Ok(())
    }

    pub fn has_running(&self) -> bool {
        !self.handles().is_empty()
    }

    fn handles(&self) -> MutexGuard<'_, Handles> {
        self.handles.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn unregister(&self, id: usize) {
        self.handles().shift_remove(&id);
        if let Some(timer) = self
            .timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id)
        {
            // Clearing a timer that already fired is harmless, as with clearTimeout.
            let _ = timer.send(());
        }
    }
}
impl Default for ToolDetachRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// An unref'ed timer does not keep the process alive; its callback still owns its inputs.
fn spawn_auto_detach_timer(
    auto_detach_after_ms: f64,
    id: usize,
    handle: Arc<dyn ToolDetachHandle>,
    handles: SharedHandles,
    cancelled: std::sync::mpsc::Receiver<()>,
    timer_reference: Sender<()>,
) {
    let delay = pi_js_compat::node_timers::normalize_timeout_delay(auto_detach_after_ms);
    let deadline = Instant::now() + Duration::from_secs_f64(delay / 1000.0);
    std::thread::spawn(move || {
        // Frozen :51 overwrites only the latest timer reference, not the earlier timer.
        // A sender owned by the timer prevents map replacement from cancelling it.
        let _timer_reference = timer_reference;
        if cancelled.recv_timeout(deadline.saturating_duration_since(Instant::now())) != Err(RecvTimeoutError::Timeout)
        {
            return;
        }
        let registered = handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(&id);
        if registered && let Err(error) = handle.detach() {
            // Actual frozen timer process probe exits 1; this is not a caught registry call.
            pi_coding_agent_base::utils::uncaught::throw_callback_uncaught(error);
        }
    });
}

pub struct DetachSubscription {
    registry: Arc<ToolDetachRegistry>,
    id: usize,
}
impl DetachSubscription {
    pub fn unregister(&self) {
        self.registry.unregister(self.id);
    }
}
impl Drop for DetachSubscription {
    fn drop(&mut self) {
        self.unregister();
    }
}
