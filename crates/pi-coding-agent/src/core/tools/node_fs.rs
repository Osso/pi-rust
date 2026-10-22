//! Node filesystem messages shared with other native packages.

pub use pi_js_compat::node_fs::{node_fs_error, node_promises_read_error, node_read_error};

use pi_js_compat::js_json::JsString;
use std::io;

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
