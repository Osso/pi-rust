//! Bundled `session-id` extension (`extensions/session-id/src/index.ts`).

use std::sync::Arc;

use crate::core::extensions::types::{ExtensionApi, HandlerError, NotifyType};
use crate::core::session_manager::lock_session_manager;

pub fn session_id_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "session-id",
        Some("Show the current session id".to_owned()),
        None,
        Arc::new(|_args, ctx| {
            Box::pin(async move {
                let session_id = lock_session_manager(&ctx.session_manager()?)
                    .get_session_id()
                    .to_owned();
                ctx.ui()?
                    .notify(&format!("Session ID: {session_id}").into(), Some(NotifyType::Info));
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
