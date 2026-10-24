//! Node child-process string validation before native UTF-8 encoding or exec.

use crate::js_json::JsString;
use crate::node_error::NativeNodeError;

/// Node v26.11.1 `lib/internal/child_process.js`, `ChildProcess.prototype.spawn`:
/// only these uv_spawn failures emit `error` on the next tick; others throw.
pub fn spawn_error_is_async(error: &std::io::Error) -> bool {
    let Some(code) = error.raw_os_error() else {
        return false;
    };
    [
        rustix::io::Errno::ACCESS,
        rustix::io::Errno::AGAIN,
        rustix::io::Errno::MFILE,
        rustix::io::Errno::NFILE,
        rustix::io::Errno::NOENT,
    ]
    .iter()
    .any(|errno| errno.raw_os_error() == code)
}

/// `validateArgumentNullCheck`, including cwd/env labels supplied by callers.
pub fn validate_string_argument(
    name: impl Into<JsString>,
    reason: &str,
    value: &JsString,
) -> Result<(), Box<NativeNodeError>> {
    if value.contains('\0') {
        return Err(Box::new(NativeNodeError::invalid_argument_value(name, reason, value)));
    }
    Ok(())
}

/// Typed string file/argv inputs: Node checks the file before argv, in index order.
pub fn validate_spawn_arguments(file: &JsString, args: &[JsString]) -> Result<(), Box<NativeNodeError>> {
    if file.is_empty() {
        return Err(Box::new(NativeNodeError::invalid_argument_value(
            "file",
            "cannot be empty",
            file,
        )));
    }
    validate_string_argument("file", "must be a string without null bytes", file)?;
    for (index, argument) in args.iter().enumerate() {
        validate_string_argument(
            format!("args[{index}]"),
            "must be a string without null bytes",
            argument,
        )?;
    }
    Ok(())
}
