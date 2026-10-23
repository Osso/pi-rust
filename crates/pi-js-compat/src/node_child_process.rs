//! Node child-process string validation before native UTF-8 encoding or exec.

use crate::js_json::JsString;
use crate::node_error::NativeNodeError;

/// `validateArgumentNullCheck`, including cwd/env labels supplied by callers.
pub fn validate_string_argument(name: &str, reason: &str, value: &JsString) -> Result<(), Box<NativeNodeError>> {
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
            &format!("args[{index}]"),
            "must be a string without null bytes",
            argument,
        )?;
    }
    Ok(())
}
