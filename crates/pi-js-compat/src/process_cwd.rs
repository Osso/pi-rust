//! Node 26.11.1 process cwd/chdir state.
//!
//! `lib/internal/bootstrap/switches/does_own_process_state.js:126–143`
//! caches cwd and clears it only after a successful native chdir.

use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::js_json::{JsString, JsValue, encode_node_utf8};
use crate::node_error::NativeNodeError;
use crate::node_fs::{decode_node_path, node_fs_dest_error, node_fs_error_record};

fn lock_cached_cwd() -> Result<MutexGuard<'static, Option<JsString>>, NativeNodeError> {
    static CWD: OnceLock<Mutex<Option<JsString>>> = OnceLock::new();
    CWD.get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| NativeNodeError::new("Error", "Process working-directory state is poisoned"))
}

/// `process.cwd()`: successful values remain cached until successful `chdir`.
pub fn cwd() -> Result<JsString, NativeNodeError> {
    let mut cached = lock_cached_cwd()?;
    if let Some(cwd) = &*cached {
        return Ok(cwd.clone());
    }
    let path = std::env::current_dir()
        .map_err(|error| NativeNodeError::from(node_fs_error_record(&error, "uv_cwd", &JsString::default())))?;
    let cwd = decode_node_path(path.as_os_str());
    *cached = Some(cwd.clone());
    Ok(cwd)
}

/// `process.chdir()`: Node's native binding uses the UTF-8 C-string prefix.
pub fn chdir(directory: &JsString) -> Result<(), NativeNodeError> {
    let mut cached = lock_cached_cwd()?;
    let encoded = encode_node_utf8(directory);
    let end = encoded.find('\0').unwrap_or(encoded.len());
    let path = &encoded[..end];
    match std::env::set_current_dir(path) {
        Ok(()) => {
            *cached = None;
            Ok(())
        }
        Err(error) => Err(chdir_error(&error, path)),
    }
}

fn chdir_error(error: &std::io::Error, destination: &str) -> NativeNodeError {
    // Node's native binding retains an empty old path when raw uv_cwd fails.
    let previous = match std::env::current_dir() {
        Ok(path) => decode_node_path(path.as_os_str()),
        Err(_) => JsString::default(),
    };
    let destination = JsString::from(destination);
    let mut error_value = NativeNodeError::from(node_fs_error_record(error, "chdir", &previous));
    error_value.message = node_fs_dest_error(error, "chdir", &previous, &destination);
    error_value.with_property("dest", JsValue::String(destination))
}
