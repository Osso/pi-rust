//! Port of `packages/agent/src/harness/session/jsonl/errors.ts`.

use crate::harness::session::types::{JsonlDecodeError, SessionError, SessionErrorCause, SessionErrorCode};
use crate::harness::types::{FileError, FileErrorCode};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

/// Map a filesystem result into a session result, preserving the failure cause.
pub fn file_result<T>(result: Result<T, FileError>, message: impl Into<JsString>) -> Result<T, SessionError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let code = if error.code == FileErrorCode::NotFound {
                SessionErrorCode::NotFound
            } else {
                SessionErrorCode::Storage
            };
            Err(SessionError::with_cause(
                code,
                message.into() + ": " + &error.message,
                SessionErrorCause::File {
                    code: error.code,
                    message: error.message,
                    path: error.path,
                },
            ))
        }
    }
}

/// Build the invalid-file error for a decode failure.
pub fn invalid_file(path: &JsString, line: usize, cause: &JsonlDecodeError) -> SessionError {
    SessionError::with_cause(
        SessionErrorCode::InvalidEntry,
        js_format!(
            "Invalid JSONL v4 session {}: line {} {}",
            path,
            line.to_string(),
            &cause.message
        ),
        SessionErrorCause::Decode {
            kind: cause.kind,
            message: cause.message.clone(),
        },
    )
}

/// Build the invalid-file error for a rejected replay mutation.
pub fn invalid_mutation_file(path: &JsString, line: usize, cause: &SessionError) -> SessionError {
    SessionError::with_cause(
        SessionErrorCode::InvalidEntry,
        js_format!(
            "Invalid JSONL v4 session {}: line {} {}",
            path,
            line.to_string(),
            cause.message()
        ),
        SessionErrorCause::Session(Box::new(cause.clone())),
    )
}
