//! Frozen session/repo-utils.ts.

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use super::facade::Session;
use super::types::*;
use crate::harness::types::{FileError, FileErrorCode, OperationFailure};

pub fn create_session_id() -> JsString {
    super::uuid::uuidv7().into()
}
pub fn create_timestamp() -> JsString {
    (system_clock())()
}
pub fn to_session<M: SessionMetadataValue>(storage: Arc<dyn SessionStorage<M>>) -> Session<M> {
    Session::new(storage)
}

pub fn get_file_system_result_or_throw<T, E>(result: Result<T, E>, message: &JsString) -> Result<T, SessionFailure>
where
    E: Into<OperationFailure<FileError>>,
{
    result.map_err(|error| match error.into() {
        OperationFailure::Rejected(error) => OperationFailure::Rejected(error),
        OperationFailure::Returned(error) => {
            let code = if error.code == FileErrorCode::NotFound {
                SessionErrorCode::NotFound
            } else {
                SessionErrorCode::Storage
            };
            OperationFailure::Returned(SessionError::with_cause(
                code,
                js_format!("{}: {}", message, error.message),
                error,
            ))
        }
    })
}

pub async fn get_entries_to_fork<M: SessionMetadataValue>(
    storage: &dyn SessionStorage<M>,
    options: &SessionForkOptions,
) -> Result<Vec<SessionTreeEntry>, SessionError> {
    let Some(id) = options.entry_id.as_ref().filter(|id| !id.is_empty()) else {
        return storage.get_entries().await;
    };
    let target = storage
        .get_entry(id)
        .await?
        .ok_or_else(|| SessionError::missing(SessionErrorCode::InvalidForkTarget, id))?;
    let leaf_id = if options.position == ForkPosition::At {
        Some(target.id())
    } else {
        if !target.message().is_some_and(|message| message.has_role("user")) {
            return Err(SessionError::new(
                SessionErrorCode::InvalidForkTarget,
                js_format!("Entry {} is not a user message", id),
            ));
        }
        target.parent_id()
    };
    storage.get_path_to_root(leaf_id).await
}
