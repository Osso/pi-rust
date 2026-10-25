//! Bundled `session-id` extension (`extensions/session-id/src/index.ts`).

use pi_js_compat::js_format;
use std::sync::Arc;

use crate::core::extensions::types::{ExtensionApi, HandlerError, NotifyType};

pub fn session_id_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "session-id",
        Some("Show the current session id".into()),
        None,
        Arc::new(|_args, ctx| {
            Box::pin(async move {
                let session_id = ctx.session_manager()?.get_session_id().to_owned();
                ctx.ui()?
                    .notify(&js_format!("Session ID: {}", session_id), Some(NotifyType::Info))?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
