use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

/// `RunStartEvent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStartEvent {
    pub lane: String,
    pub run_id: String,
}

/// `RunEndEvent.outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunEndOutcome {
    Completed,
    Aborted,
    Failed,
}

/// `HarnessEvent`: the closed union of observable harness run events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum HarnessEvent {
    RunStart(RunStartEvent),
    RunEnd {
        lane: String,
        run_id: String,
        outcome: RunEndOutcome,
        leaf_id: String,
    },
}

/// `HarnessEventType`: the event `type` discriminator.
pub type HarnessEventType = &'static str;

impl HarnessEvent {
    /// Returns the exact source `type` literal.
    pub const fn event_type(&self) -> HarnessEventType {
        match self {
            Self::RunStart(_) => "run_start",
            Self::RunEnd { .. } => "run_end",
        }
    }
}

/// `HarnessEventListener`: passive callback invoked synchronously by [`HarnessEventBus::emit`].
pub type HarnessEventListener = Arc<dyn Fn(&HarnessEvent) + Send + Sync>;

/// Handle returned by [`HarnessEventBus::on`]; dropping it does not unsubscribe.
pub struct EventSubscription {
    bus: HarnessEventBus,
    id: u64,
}

impl EventSubscription {
    /// Removes the listener; repeated calls are harmless.
    pub fn unsubscribe(self) {
        self.bus.inner.remove_listener(self.id);
    }
}

/// `WatchHandle`: snapshot captured at registration plus buffered and live events.
pub struct WatchHandle<TSnapshot> {
    bus: HarnessEventBus,
    id: u64,
    snapshot: TSnapshot,
    state: Arc<Mutex<WatchState>>,
}

struct WatchState {
    listener: Option<HarnessEventListener>,
    buffered: Vec<HarnessEvent>,
}

impl<TSnapshot> WatchHandle<TSnapshot> {
    /// Snapshot captured when the watch was created.
    pub fn snapshot(&self) -> &TSnapshot {
        &self.snapshot
    }

    /// Flushes buffered events to `listener`, then delivers live events.
    pub fn start(&mut self, listener: HarnessEventListener) {
        loop {
            let pending = {
                let mut state = lock(&self.state);
                std::mem::take(&mut state.buffered)
            };
            if pending.is_empty() {
                break;
            }
            for event in &pending {
                listener(event);
            }
        }
        lock(&self.state).listener = Some(listener);
    }

    /// Stops delivery and drops buffered events; repeated calls are harmless.
    pub fn unsubscribe(&mut self) {
        self.bus.inner.remove_watch(self.id);
        let mut state = lock(&self.state);
        state.buffered.clear();
        state.listener = None;
    }
}

struct EventBusInner {
    listeners: Mutex<HashMap<HarnessEventType, Vec<(u64, HarnessEventListener)>>>,
    watch_listeners: Mutex<Vec<(u64, Arc<Mutex<WatchState>>)>>,
    next_id: AtomicU64,
}

impl EventBusInner {
    fn remove_listener(&self, id: u64) {
        let mut listeners = lock(&self.listeners);
        for entries in listeners.values_mut() {
            entries.retain(|(entry_id, _)| *entry_id != id);
        }
        listeners.retain(|_, entries| !entries.is_empty());
    }

    fn remove_watch(&self, id: u64) {
        lock(&self.watch_listeners).retain(|(entry_id, _)| *entry_id != id);
    }
}

/// `HarnessEventBus`: typed event fan-out with snapshot watches.
///
/// Direct listeners and watchers receive events synchronously in registration order. No lock is held
/// while a listener runs, so listeners may emit reentrantly.
#[derive(Clone)]
pub struct HarnessEventBus {
    inner: Arc<EventBusInner>,
}

impl Default for HarnessEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl HarnessEventBus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(EventBusInner {
                listeners: Mutex::new(HashMap::new()),
                watch_listeners: Mutex::new(Vec::new()),
                next_id: AtomicU64::new(1),
            }),
        }
    }

    /// Registers a listener for future events of `event_type`; earlier events are not replayed.
    pub fn on(&self, event_type: HarnessEventType, listener: HarnessEventListener) -> EventSubscription {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        lock(&self.inner.listeners)
            .entry(event_type)
            .or_default()
            .push((id, listener));
        EventSubscription { bus: self.clone(), id }
    }

    /// Publishes an event to current direct listeners and watchers.
    pub fn emit(&self, event: HarnessEvent) {
        let direct: Vec<HarnessEventListener> = lock(&self.inner.listeners)
            .get(event.event_type())
            .map(|entries| entries.iter().map(|(_, listener)| Arc::clone(listener)).collect())
            .unwrap_or_default();
        for listener in direct {
            listener(&event);
        }

        let watchers: Vec<Arc<Mutex<WatchState>>> = lock(&self.inner.watch_listeners)
            .iter()
            .map(|(_, state)| Arc::clone(state))
            .collect();
        for state in watchers {
            receive(&state, &event);
        }
    }

    /// Registers a watcher and captures its snapshot after registration, so events emitted during
    /// capture are buffered instead of lost.
    pub fn watch<TSnapshot>(&self, capture_snapshot: impl FnOnce() -> TSnapshot) -> WatchHandle<TSnapshot> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let state = Arc::new(Mutex::new(WatchState {
            listener: None,
            buffered: Vec::new(),
        }));
        lock(&self.inner.watch_listeners).push((id, Arc::clone(&state)));
        let snapshot = capture_snapshot();
        WatchHandle {
            bus: self.clone(),
            id,
            snapshot,
            state,
        }
    }
}

fn receive(state: &Mutex<WatchState>, event: &HarnessEvent) {
    let listener = {
        let mut state = lock(state);
        match &state.listener {
            Some(listener) => Some(Arc::clone(listener)),
            None => {
                state.buffered.push(event.clone());
                None
            }
        }
    };
    if let Some(listener) = listener {
        listener(event);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
