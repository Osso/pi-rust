//! Process-level uncaught errors.
//!
//! TypeScript code that throws with no `try/catch`, rejects a promise nobody
//! awaits, or fails inside a timer callback reaches Node's default uncaught
//! handler: it prints `<Error name>: <message>` and exits 1. Rust has no
//! exceptions, so the port routes those sites through this module and the
//! `pi` binary installs [`install_uncaught_handler`] as the panic hook.
//!
//! While interactive mode owns the terminal it installs a
//! [`ThreadCrashHandler`] (`registerSignalHandlers`' `uncaughtException`
//! listener). Uncaught throws and panics on that thread then run the handler
//! instead of the default report, so `ui.stop()` can restore cooked mode and
//! the cursor. Clearing the handler restores the default report.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Node's default label for a plain `Error`.
pub const ERROR: &str = "Error";

/// A thrown value carrying the Node error name that prefixes the message.
#[derive(Debug)]
pub struct UncaughtError {
    pub label: &'static str,
    pub message: String,
}

/// Process-level handler installed for one thread (interactive mode).
pub type ThreadCrashHandler = Rc<dyn Fn(&(dyn Any + Send))>;

thread_local! {
    static THREAD_CRASH_HANDLER: RefCell<Option<ThreadCrashHandler>> = const { RefCell::new(None) };
    /// Guards against a handler panic re-entering the hook forever. Rust has
    /// no analogue of a second uncaught exception inside `uncaughtCrash`.
    static HANDLING_CRASH: Cell<bool> = const { Cell::new(false) };
}

/// Installs `handler` for the current thread and returns the previous one.
pub fn set_thread_crash_handler(handler: Option<ThreadCrashHandler>) -> Option<ThreadCrashHandler> {
    THREAD_CRASH_HANDLER.with(|slot| slot.replace(handler))
}

/// The crash handler installed for the current thread, if any.
pub fn thread_crash_handler() -> Option<ThreadCrashHandler> {
    THREAD_CRASH_HANDLER.with(|slot| slot.borrow().clone())
}

/// `throw` an error that nothing catches.
pub fn throw_uncaught(label: &'static str, message: impl Into<String>) -> ! {
    let error = UncaughtError {
        label,
        message: message.into(),
    };
    if let Some(handler) = thread_crash_handler() {
        handler(&error);
    }
    std::panic::panic_any(error)
}

/// Formats a panic payload the way Node's uncaught handler prints it.
pub fn format_uncaught(payload: &(dyn Any + Send)) -> String {
    if let Some(error) = payload.downcast_ref::<UncaughtError>() {
        return format!("{}: {}", error.label, error.message);
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return format!("{ERROR}: {message}");
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return format!("{ERROR}: {message}");
    }
    format!("{ERROR}: unknown panic payload")
}

/// Installs the process-level handler: print the error, exit 1 (not 101).
pub fn install_uncaught_handler() {
    std::panic::set_hook(Box::new(|info| {
        if HANDLING_CRASH.get() {
            eprintln!("{}", format_uncaught(info.payload()));
            std::process::exit(1);
        }
        if let Some(handler) = thread_crash_handler() {
            HANDLING_CRASH.set(true);
            handler(info.payload());
        }
        eprintln!("{}", format_uncaught(info.payload()));
        std::process::exit(1);
    }));
}
