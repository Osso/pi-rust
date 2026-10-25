//! Channel event bus (`core/event-bus.ts`).

use std::any::Any;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use futures_util::future::BoxFuture;

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, JsValue};

pub type Handler = dyn Fn(&JsValue) -> Result<(), CallbackError> + Send + Sync;
pub type RequestHandler = dyn Fn(&mut EventRequest) -> Result<(), CallbackError> + Send + Sync;
pub type RequestFuture = BoxFuture<'static, Result<JsValue, JsString>>;

/// Native counterpart of a mutable JS request containing a promise field.
/// Listeners assign `response` synchronously; the emitter awaits the last claim.
pub struct EventRequest {
    pub data: JsValue,
    pub context: Option<Arc<dyn Any + Send + Sync>>,
    pub response: Option<RequestFuture>,
}

impl EventRequest {
    pub fn new(data: JsValue) -> Self {
        Self {
            data,
            context: None,
            response: None,
        }
    }
}

#[derive(Clone)]
enum ListenerHandler {
    Event(Arc<Handler>),
    Request(Arc<RequestHandler>),
}

impl ListenerHandler {
    fn dispatch(&self, request: &mut EventRequest) -> Result<(), CallbackError> {
        match self {
            Self::Event(handler) => handler(&request.data),
            Self::Request(handler) => handler(request),
        }
    }
}

/// A handler failure. The source logs it to stderr and keeps dispatching;
/// callers that need the diagnosis receive it from [`EventBus::emit`].
#[derive(Debug, Clone, PartialEq)]
pub struct HandlerError {
    pub channel: JsString,
    pub error: CallbackError,
}

pub trait EventBus: Send + Sync {
    fn emit(&self, channel: &JsString, data: JsValue) -> Vec<HandlerError>;
    fn on(&self, channel: &JsString, handler: Arc<Handler>) -> Subscription;
    fn emit_request(&self, channel: &JsString, request: &mut EventRequest) -> Vec<HandlerError>;
    fn on_request(&self, channel: &JsString, handler: Arc<RequestHandler>) -> Subscription;
}

pub struct EventBusController {
    listeners: Arc<Mutex<Vec<Listener>>>,
}

struct Listener {
    id: u64,
    channel: JsString,
    handler: ListenerHandler,
}

impl EventBusController {
    pub fn new() -> Self {
        Self {
            listeners: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a `Send + Sync` closure without wrapping it in an `Arc` at the call site.
    pub fn on_handler<H>(&self, channel: &JsString, handler: H) -> Subscription
    where
        H: Fn(&JsValue) -> Result<(), CallbackError> + Send + Sync + 'static,
    {
        self.on(channel, Arc::new(handler))
    }

    pub fn clear(&self) {
        self.listeners().clear();
    }

    fn subscribe(&self, channel: &JsString, handler: ListenerHandler) -> Subscription {
        let mut listeners = self.listeners();
        let id = listeners.iter().map(|listener| listener.id).max().unwrap_or(0) + 1;
        listeners.push(Listener {
            id,
            channel: channel.clone(),
            handler,
        });
        Subscription {
            listeners: Arc::downgrade(&self.listeners),
            id,
        }
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
    fn emit(&self, channel: &JsString, data: JsValue) -> Vec<HandlerError> {
        self.emit_request(channel, &mut EventRequest::new(data))
    }

    fn emit_request(&self, channel: &JsString, request: &mut EventRequest) -> Vec<HandlerError> {
        // Snapshot under the lock, then deliver without it: a handler may emit
        // recursively or unsubscribe, exactly like the source's EventEmitter.
        let handlers: Vec<ListenerHandler> = {
            let listeners = self.listeners();
            listeners
                .iter()
                .filter(|listener| &listener.channel == channel)
                .map(|listener| listener.handler.clone())
                .collect()
        };
        handlers
            .iter()
            .filter_map(|handler| {
                handler.dispatch(request).err().map(|error| {
                    // Frozen event-bus.ts:19-23 catches/logs the original throw and keeps dispatching.
                    crate::core::desktop_notification::report_callback_error(
                        pi_js_compat::js_format!("Event handler error ({}):", channel),
                        &error,
                    );
                    HandlerError {
                        channel: channel.clone(),
                        error,
                    }
                })
            })
            .collect()
    }

    fn on(&self, channel: &JsString, handler: Arc<Handler>) -> Subscription {
        self.subscribe(channel, ListenerHandler::Event(handler))
    }

    fn on_request(&self, channel: &JsString, handler: Arc<RequestHandler>) -> Subscription {
        self.subscribe(channel, ListenerHandler::Request(handler))
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
