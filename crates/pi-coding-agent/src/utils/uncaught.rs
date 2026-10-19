//! Process-level uncaught errors.
//!
//! TypeScript code that throws with no `try/catch`, rejects a promise nobody
//! awaits, or fails inside a timer callback reaches Node's default uncaught
//! handler: it prints `<Error name>: <message>` and exits 1. Rust has no
//! exceptions, so the port routes those sites through this module and the
//! `pi` binary installs [`install_uncaught_handler`] as the panic hook.

use std::any::Any;

/// Node's default label for a plain `Error`.
pub const ERROR: &str = "Error";

/// A thrown value carrying the Node error name that prefixes the message.
#[derive(Debug)]
pub struct UncaughtError {
    pub label: &'static str,
    pub message: String,
}

/// `throw` an error that nothing catches.
pub fn throw_uncaught(label: &'static str, message: impl Into<String>) -> ! {
    std::panic::panic_any(UncaughtError {
        label,
        message: message.into(),
    })
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
        eprintln!("{}", format_uncaught(info.payload()));
        std::process::exit(1);
    }));
}
