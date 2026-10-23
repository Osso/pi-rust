//! Process-level uncaught errors.
//!
//! TypeScript code that throws with no `try/catch`, rejects a promise nobody
//! awaits, or fails inside a timer callback reaches Node's default uncaught
//! handler: it prints `<Error name>: <message>` and exits 1. Rust has no
//! exceptions, so the port routes those sites through this module and the
//! `pi` binary installs [`install_uncaught_handler`] as the panic hook.
//!
//! While interactive mode owns the terminal it installs a
//! [`ProcessCrashHandler`] (`registerSignalHandlers`' `uncaughtException`
//! listener). Uncaught throws and panics on any thread run that handler,
//! using a Send-able terminal restore plan. Clearing it restores the default report.

use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use std::any::Any;
use std::cell::Cell;
use std::sync::{Arc, Mutex};

/// Node's default label for a plain `Error`.
pub const ERROR: &str = "Error";

/// A thrown value carrying the Node error name that prefixes the message.
#[derive(Debug)]
pub struct UncaughtError {
    pub label: &'static str,
    pub message: JsString,
}

/// Process-level handler installed while interactive mode owns the terminal.
pub type ProcessCrashHandler = Arc<dyn Fn(&(dyn Any + Send)) + Send + Sync>;

static PROCESS_CRASH_HANDLER: Mutex<Option<ProcessCrashHandler>> = Mutex::new(None);

thread_local! {
    /// Guards against a handler panic re-entering the hook forever. Rust has
    /// no analogue of a second uncaught exception inside `uncaughtCrash`.
    static HANDLING_CRASH: Cell<bool> = const { Cell::new(false) };
    /// Set while [`swallow_panics`] runs: the hook stays silent so the panic
    /// unwinds into `catch_unwind`.
    static SWALLOWING: Cell<bool> = const { Cell::new(false) };
}

/// Installs `handler` process-wide and returns the previous one.
pub fn set_process_crash_handler(handler: Option<ProcessCrashHandler>) -> Option<ProcessCrashHandler> {
    std::mem::replace(
        &mut *PROCESS_CRASH_HANDLER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        handler,
    )
}

/// Clone before dispatch so the handler can unregister itself without deadlock.
pub fn process_crash_handler() -> Option<ProcessCrashHandler> {
    PROCESS_CRASH_HANDLER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// `throw` an error that nothing catches.
pub fn throw_uncaught(label: &'static str, message: impl Into<JsString>) -> ! {
    std::panic::panic_any(UncaughtError {
        label,
        message: message.into(),
    })
}

/// Routes a native throw only after it reaches an uncaught process/callback boundary.
pub fn throw_native_uncaught(error: pi_js_compat::node_error::NativeNodeError) -> ! {
    std::panic::panic_any(error)
}

/// `try { step } catch {}`: runs `step` and discards a panic from it. The
/// hook runs before unwinding, so it must know the panic is expected.
pub fn swallow_panics(step: impl FnOnce()) {
    let _ = catch_panics(step);
}

/// Catch a renderer throw without invoking the process-level uncaught handler.
pub fn catch_panics<T>(step: impl FnOnce() -> T) -> Result<T, Box<dyn Any + Send>> {
    let previous = SWALLOWING.replace(true);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(step));
    SWALLOWING.set(previous);
    result
}

/// Formats a panic payload the way Node's uncaught handler prints it.
pub fn format_uncaught(payload: &(dyn Any + Send)) -> JsString {
    if let Some(error) = payload.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
        return error.string_value();
    }
    if let Some(error) = payload.downcast_ref::<UncaughtError>() {
        return js_format!("{}: {}", error.label, error.message);
    }
    if let Some(error) = payload.downcast_ref::<std::io::Error>() {
        return format!("{ERROR}: {error}").into();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return js_format!("{}: {}", ERROR, message);
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return js_format!("{}: {}", ERROR, message);
    }
    js_format!("{}: unknown panic payload", ERROR)
}

/// Installs the process-level handler: print the error, exit 1 (not 101).
pub fn install_uncaught_handler() {
    std::panic::set_hook(Box::new(|info| {
        if SWALLOWING.get() {
            return;
        }
        if !HANDLING_CRASH.replace(true)
            && let Some(handler) = process_crash_handler()
        {
            handler(info.payload());
        }
        eprintln!("{}", encode_node_utf8(&format_uncaught(info.payload())));
        std::process::exit(1);
    }));
}
