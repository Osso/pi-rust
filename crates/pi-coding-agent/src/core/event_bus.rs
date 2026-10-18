//! Channel event bus (`core/event-bus.ts`).

use std::sync::{Arc, Mutex, MutexGuard, Weak};

use serde_json::Value;

pub type Handler = dyn Fn(&Value) -> Result<(), String> + Send + Sync;

/// A handler failure. The source logs it to stderr and keeps dispatching;
/// callers that need the diagnosis receive it from [`EventBus::emit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerError {
    pub channel: String,
    pub message: String,
}

pub trait EventBus: Send + Sync {
    fn emit(&self, channel: &str, data: Value) -> Vec<HandlerError>;
    fn on(&self, channel: &str, handler: Arc<Handler>) -> Subscription;
}

pub struct EventBusController {
    listeners: Arc<Mutex<Vec<Listener>>>,
}

struct Listener {
    id: u64,
    channel: String,
    handler: Arc<Handler>,
}

impl EventBusController {
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a `Send + Sync` closure without wrapping it in an `Arc` at the call site.
    pub fn on_handler<H>(&self, channel: &str, handler: H) -> Subscription
    where
        H: Fn(&Value) -> Result<(), String> + Send + Sync + 'static,
    {
        self.on(channel, Arc::new(handler))
    }

    pub fn clear(&self) {
        self.listeners().clear();
    }

    fn listeners(&self) -> MutexGuard<'_, Vec<Listener>> {
        self.listeners.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for EventBusController {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus for EventBusController {
    fn emit(&self, channel: &str, data: Value) -> Vec<HandlerError> {
        // Snapshot under the lock, then deliver without it: a handler may emit
        // recursively or unsubscribe, exactly like the source's EventEmitter.
        let handlers: Vec<Arc<Handler>> = {
            let listeners = self.listeners();
            listeners
                .iter()
                .filter(|listener| listener.channel == channel)
                .map(|listener| Arc::clone(&listener.handler))
                .collect()
        };
        handlers
            .iter()
            .filter_map(|handler| {
                handler(&data).err().map(|message| HandlerError {
                    channel: channel.to_owned(),
                    message,
                })
            })
            .collect()
    }

    fn on(&self, channel: &str, handler: Arc<Handler>) -> Subscription {
        let mut listeners = self.listeners();
        let id = listeners.iter().map(|listener| listener.id).max().unwrap_or(0) + 1;
        listeners.push(Listener {
            id,
            channel: channel.to_owned(),
            handler,
        });
        Subscription {
            listeners: Arc::downgrade(&self.listeners),
            id,
        }
    }
}

pub struct Subscription {
    listeners: Weak<Mutex<Vec<Listener>>>,
    id: u64,
}

impl Subscription {
    pub fn unsubscribe(self) {
        self.unsubscribe_ref();
    }

    fn unsubscribe_ref(&self) {
        if let Some(listeners) = self.listeners.upgrade() {
            listeners
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .retain(|listener| listener.id != self.id);
        }
    }
}

pub fn create_event_bus() -> EventBusController {
    EventBusController::new()
}
