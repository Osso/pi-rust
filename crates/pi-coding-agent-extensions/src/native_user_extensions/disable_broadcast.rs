//! Port of the user's `disable-broadcast.ts`.

use crate::core::extensions::{BoxFuture, ExtensionApi, ExtensionContext, HandlerError};

pub fn disable_broadcast_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let pi_for_start = pi.clone();
    pi.on_session_start(
        move |_event, _ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let pi = pi_for_start.clone();
            Box::pin(async move {
                let active = pi
                    .get_active_tools()?
                    .into_iter()
                    .filter(|name| name != "broadcast")
                    .collect();
                pi.set_active_tools(active)?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
