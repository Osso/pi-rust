//! Node filesystem messages shared with other native packages.

pub use pi_js_compat::node_fs::{node_fs_error, node_promises_read_error, node_read_error};

use pi_js_compat::js_json::JsString;
use std::io;

/// Preserve native messages through Operations traits, whose error carrier is IO.
pub fn operation_error(message: impl Into<JsString>) -> io::Error {
    io::Error::other(crate::core::extensions::types::HandlerError::new(message))
}

/// Reject invalid Node path arguments before OS encoding or syscall entry.
pub fn validate_node_path(path: &JsString) -> io::Result<()> {
    match pi_js_compat::node_fs::node_path_argument_error(path) {
        Some(error) => Err(operation_error(error.message)),
        None => Ok(()),
    }
}

/// Recover native rejection data through IO handler/callback carrier sources.
pub fn operation_native_error(error: &io::Error) -> Option<&pi_js_compat::node_error::NativeNodeError> {
    let mut source: &(dyn std::error::Error + 'static) = error.get_ref()?;
    loop {
        if let Some(native) = source.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
            return Some(native);
        }
        source = source.source()?;
    }
}

/// Custom sandbox operations carry native extension errors through the IO trait.
/// Only actual OS errors use their already-scalar filesystem diagnostic.
pub fn operation_error_message(error: &io::Error) -> JsString {
    if let Some(native) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<crate::core::extensions::types::HandlerError>())
    {
        return native.message.clone();
    }
    error.to_string().into()
}
