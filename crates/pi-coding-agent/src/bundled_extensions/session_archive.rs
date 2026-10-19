//! Bundled `session-archive` extension (`extensions/session-archive/src/index.ts`).

use std::sync::Arc;

use crate::core::extensions::types::{ExtensionApi, HandlerError, NotifyType};
use crate::core::session_control_db::archive_session;

pub fn session_archive_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "archive",
        Some("Archive the current session".to_owned()),
        None,
        Arc::new(|args, ctx| {
            Box::pin(async move {
                if !args.trim().is_empty() {
                    ctx.ui()?.notify("Usage: /archive", Some(NotifyType::Warning));
                    return Ok(());
                }
                let Some(control_db_path) = ctx.control_db_path()? else {
                    ctx.ui()?
                        .notify("Session archive requires a control database.", Some(NotifyType::Error));
                    return Ok(());
                };
                let session_path = ctx.session_manager()?.get_session_file().map(str::to_owned);
                let Some(session_path) = session_path else {
                    ctx.ui()?
                        .notify("The current session is not persisted.", Some(NotifyType::Warning));
                    return Ok(());
                };

                archive_session(&control_db_path, &session_path)
                    .map_err(|error| HandlerError::new(error.to_string()))?;
                ctx.ui()?.notify("Archived current session.", Some(NotifyType::Info));
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
