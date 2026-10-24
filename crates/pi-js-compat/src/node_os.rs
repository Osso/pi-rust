//! Shared POSIX Node `os.homedir()` and `os.tmpdir()` authorities.

use crate::{
    js_format,
    js_json::{JsObject, JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
    node_fs::decode_node_path,
};
use nix::{
    errno::Errno,
    unistd::{Uid, User},
};

/// Like Node, a missing passwd record or failed lookup throws; USERPROFILE is not a POSIX fallback.
pub fn homedir() -> Result<JsString, NativeNodeError> {
    if let Some(home) = crate::process_env::get_native("HOME") {
        return Ok(home);
    }
    match User::from_uid(Uid::effective()) {
        Ok(Some(user)) => Ok(decode_node_path(user.dir.as_os_str())),
        Ok(None) => Err(passwd_error(Errno::ENOENT)),
        Err(error) => Err(passwd_error(error)),
    }
}

/// Node v26.11.1 `lib/os.js::tmpdir` / POSIX getTempDir binding.
/// Reads TMPDIR || TMP || TEMP || '/tmp', then removes exactly one trailing slash
/// from non-root values, retaining native UTF-16 through the path boundary.
pub fn tmpdir() -> JsString {
    let path = ["TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .find_map(|key| crate::process_env::get_native(key).filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "/tmp".into());
    if path.len() > 1 && path.ends_with('/') {
        path.slice(0, path.len() - 1)
    } else {
        path
    }
}

fn passwd_error(error: Errno) -> NativeNodeError {
    let code = crate::node_fs::node_error_code(&std::io::Error::from_raw_os_error(error as i32));
    let description = crate::node_fs::node_errno_description(error as i32);
    // Approved gap: nix reads errno rather than getpwuid_r's return code.
    homedir_system_error(-(error as i32), &code, &description)
}

/// Node's ERR_SYSTEM_ERROR, with the binding context retained as `info`.
pub fn homedir_system_error(errno: i32, code: &str, description: &str) -> NativeNodeError {
    let syscall = "uv_os_homedir";
    let info = JsObject::from([
        ("errno".into(), JsValue::Number(f64::from(errno))),
        ("code".into(), JsValue::String(code.into())),
        ("message".into(), JsValue::String(description.into())),
        ("syscall".into(), JsValue::String(syscall.into())),
    ]);
    NativeNodeError::from_constructor(
        NativeErrorClass::SystemError,
        js_format!(
            "A system error occurred: {} returned {} ({})",
            syscall,
            code,
            description
        ),
    )
    .with_property("code", JsValue::String("ERR_SYSTEM_ERROR".into()))
    .with_property("info", JsValue::Object(info))
    .with_property("errno", JsValue::Number(f64::from(errno)))
    .with_property("syscall", JsValue::String(syscall.into()))
}
