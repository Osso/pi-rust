//! Extension runtime state (`core/extensions/extension-runtime.ts`).

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex, RwLock};

use indexmap::IndexMap;

use crate::core::event_bus::Subscription;

use super::types::{
    ExtensionActions, ExtensionFlag, FlagValue, HandlerError, ProviderConfig, RegisterProviderHandler,
    UnregisterProviderRuntimeCallback,
};
use pi_ai::request::CallbackError;

pub(crate) const DEFAULT_STALE_MESSAGE: &str = "This extension ctx is stale after session replacement or reload. Do not use a captured pi or command ctx after ctx.newSession(), ctx.fork(), ctx.switchSession(), or ctx.reload(). For newSession, fork, and switchSession, move post-replacement work into withSession and use the ctx passed to withSession. For reload, do not use the old ctx after await ctx.reload().";

const NOT_INITIALIZED: &str =
    "Extension runtime not initialized. Action methods cannot be called during extension loading.";

#[derive(Clone)]
pub struct PendingProviderRegistration {
    pub name: String,
    pub config: ProviderConfig,
    pub extension_path: JsString,
}

/// Registration hooks installed by `ExtensionRunner::bindCore`.
#[derive(Clone)]
pub struct ProviderHandlers {
    pub register_provider: RegisterProviderHandler,
    pub unregister_provider: UnregisterProviderRuntimeCallback,
}

pub struct RuntimeInner {
    stale_message: Mutex<Option<String>>,
    flag_values: Mutex<IndexMap<String, FlagValue>>,
    pending_provider_registrations: Mutex<Vec<PendingProviderRegistration>>,
    actions: RwLock<ExtensionActions>,
    provider_handlers: RwLock<Option<ProviderHandlers>>,
    event_bus_unsubscribers: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
}

/// Runtime state plus action implementations.
#[derive(Clone)]
pub struct ExtensionRuntime {
    pub(crate) inner: Arc<RuntimeInner>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn uninitialized_actions() -> ExtensionActions {
    ExtensionActions {
        send_message: Arc::new(|_, _| Err(HandlerError::new(NOT_INITIALIZED))),
        send_user_message: Arc::new(|_, _| Err(HandlerError::new(NOT_INITIALIZED))),
        append_entry: Arc::new(|_, _| Err(HandlerError::new(NOT_INITIALIZED))),
        request_resume_continuation: Arc::new(|| Err(NOT_INITIALIZED.into())),
        set_session_name: Arc::new(|_| Err(HandlerError::new(NOT_INITIALIZED))),
        get_session_name: Arc::new(|| Err(HandlerError::new(NOT_INITIALIZED))),
        set_label: Arc::new(|_, _| Err(HandlerError::new(NOT_INITIALIZED))),
        get_active_tools: Arc::new(|| Err(HandlerError::new(NOT_INITIALIZED))),
        get_all_tools: Arc::new(|| Err(HandlerError::new(NOT_INITIALIZED))),
        set_active_tools: Arc::new(|_| Err(HandlerError::new(NOT_INITIALIZED))),
        call_tool: Arc::new(|_, _, _, _| Box::pin(async { Err("Extension runtime not initialized".into()) })),
        call_command: Arc::new(|_, _| Box::pin(async { Err("Extension runtime not initialized".into()) })),
        refresh_tools: Arc::new(|| Ok(())),
        get_commands: Arc::new(|| Err(HandlerError::new(NOT_INITIALIZED))),
        set_model: Arc::new(|_| Box::pin(async { Err("Extension runtime not initialized".into()) })),
        get_thinking_level: Arc::new(|| Err(HandlerError::new(NOT_INITIALIZED))),
        set_thinking_level: Arc::new(|_| Err(HandlerError::new(NOT_INITIALIZED))),
    }
}

impl ExtensionRuntime {
    pub(crate) fn create() -> Self {
        Self {
            inner: Arc::new(RuntimeInner {
                stale_message: Mutex::new(None),
                flag_values: Mutex::new(IndexMap::new()),
                pending_provider_registrations: Mutex::new(Vec::new()),
                actions: RwLock::new(uninitialized_actions()),
                provider_handlers: RwLock::new(None),
                event_bus_unsubscribers: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn assert_active(&self) -> Result<(), String> {
        match lock(&self.inner.stale_message).as_ref() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    pub fn invalidate(&self, message: Option<&str>) {
        let mut stale = lock(&self.inner.stale_message);
        if stale.is_some() {
            return;
        }
        *stale = Some(message.map_or_else(|| DEFAULT_STALE_MESSAGE.to_owned(), str::to_owned));
        drop(stale);
        let mut unsubscribers = lock(&self.inner.event_bus_unsubscribers);
        for unsubscribe in unsubscribers.drain(..) {
            unsubscribe();
        }
    }

    pub fn track_event_bus_subscription(&self, subscription: Subscription) -> Arc<dyn Fn() + Send + Sync> {
        let subscription = Arc::new(Mutex::new(Some(subscription)));
        let tracked_subscription = Arc::clone(&subscription);
        let tracked = Arc::new(move || {
            if let Some(subscription) = lock(&tracked_subscription).take() {
                subscription.unsubscribe();
            }
        });
        let cleanup = Arc::clone(&tracked);
        lock(&self.inner.event_bus_unsubscribers).push(Box::new(move || cleanup()));
        tracked
    }

    /// Snapshot of the bound actions; cloning keeps no lock across await points.
    pub fn actions(&self) -> ExtensionActions {
        read(&self.inner.actions).clone()
    }

    pub fn set_actions(&self, actions: ExtensionActions) {
        *write(&self.inner.actions) = actions;
    }

    pub fn flag_value(&self, name: &str) -> Option<FlagValue> {
        lock(&self.inner.flag_values).get(name).cloned()
    }

    pub fn set_flag_value(&self, name: &str, value: FlagValue) {
        lock(&self.inner.flag_values).insert(name.to_owned(), value);
    }

    pub fn set_default_flag_value(&self, name: &str, value: FlagValue) {
        let mut values = lock(&self.inner.flag_values);
        if !values.contains_key(name) {
            values.insert(name.to_owned(), value);
        }
    }

    pub fn flag_values(&self) -> IndexMap<String, FlagValue> {
        lock(&self.inner.flag_values).clone()
    }

    pub fn register_flag_definition(&self, flag: &ExtensionFlag) {
        if let Some(default) = &flag.default {
            self.set_default_flag_value(&flag.name, default.clone());
        }
    }

    pub fn pending_provider_registrations(&self) -> Vec<PendingProviderRegistration> {
        lock(&self.inner.pending_provider_registrations).clone()
    }

    pub fn clear_pending_provider_registrations(&self) {
        lock(&self.inner.pending_provider_registrations).clear();
    }

    pub fn register_provider(
        &self,
        name: &str,
        config: ProviderConfig,
        extension_path: Option<&JsString>,
    ) -> Result<(), CallbackError> {
        let handlers = read(&self.inner.provider_handlers).clone();
        if let Some(handlers) = handlers {
            return (handlers.register_provider)(name, &config, extension_path);
        }
        lock(&self.inner.pending_provider_registrations).push(PendingProviderRegistration {
            name: name.to_owned(),
            config,
            extension_path: extension_path.cloned().unwrap_or_else(|| "<unknown>".into()),
        });
        Ok(())
    }

    pub fn unregister_provider(&self, name: &str, extension_path: Option<&JsString>) -> Result<(), CallbackError> {
        let handlers = read(&self.inner.provider_handlers).clone();
        if let Some(handlers) = handlers {
            return (handlers.unregister_provider)(name, extension_path);
        }
        lock(&self.inner.pending_provider_registrations).retain(|entry| entry.name != name);
        Ok(())
    }

    pub fn set_provider_handlers(&self, handlers: ProviderHandlers) {
        *write(&self.inner.provider_handlers) = Some(handlers);
    }
}

/// Create a runtime with action stubs that `ExtensionRunner::bindCore` replaces.
pub fn create_extension_runtime() -> ExtensionRuntime {
    ExtensionRuntime::create()
}
