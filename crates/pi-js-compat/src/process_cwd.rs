//! Shared native process cwd/chdir with Node-compatible error records.

use crate::js_json::{JsString, JsValue, encode_node_utf8};
use crate::node_error::NativeNodeError;
use crate::node_fs::{decode_node_path, node_fs_dest_error, node_fs_error_record};

/// Read the actual working directory; Node's cached previous path is not emulated.
pub fn cwd() -> Result<JsString, NativeNodeError> {
    std::env::current_dir()
        .map(|path| decode_node_path(path.as_os_str()))
        .map_err(|error| NativeNodeError::from(node_fs_error_record(&error, "uv_cwd", &JsString::default())))
}

/// `process.chdir()`: Node's native binding uses the UTF-8 C-string prefix.
pub fn chdir(directory: &JsString) -> Result<(), NativeNodeError> {
    let encoded = encode_node_utf8(directory);
    let end = encoded.find('\0').unwrap_or(encoded.len());
    let path = &encoded[..end];
    std::env::set_current_dir(path).map_err(|error| chdir_error(&error, path))
}

fn chdir_error(error: &std::io::Error, destination: &str) -> NativeNodeError {
    // Approved gap (AGENTS.md): empty old path instead of Node's unstable buffer after failed uv_cwd.
    let previous = match std::env::current_dir() {
        Ok(path) => decode_node_path(path.as_os_str()),
        Err(_) => JsString::default(),
    };
    let destination = JsString::from(destination);
    let mut error_value = NativeNodeError::from(node_fs_error_record(error, "chdir", &previous));
    error_value.message = node_fs_dest_error(error, "chdir", &previous, &destination);
    error_value.with_property("dest", JsValue::String(destination))
}
