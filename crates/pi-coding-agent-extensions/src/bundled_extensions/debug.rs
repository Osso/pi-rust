//! Bundled `debug` extension (`extensions/debug/src/index.ts`).

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use crate::core::extensions::types::{BoxFuture, ExtensionApi, HandlerError, NotifyType};

/// `DebugReplController` from the extension source.
pub trait DebugReplController: Send + Sync {
    fn enable(&self, session_id: &JsString) -> BoxFuture<'static, Result<JsString, HandlerError>>;
    fn disable(&self) -> BoxFuture<'static, Result<(), HandlerError>>;
}

pub type DebugReplControllerProvider = Arc<dyn Fn() -> Arc<dyn DebugReplController> + Send + Sync>;

pub fn debug_extension(pi: &ExtensionApi, get_controller: DebugReplControllerProvider) -> Result<(), HandlerError> {
    pi.register_command(
        "debug",
        Some("Enable or disable the privileged live-process debug REPL".into()),
        None,
        Arc::new(move |args, ctx| {
            let get_controller = Arc::clone(&get_controller);
            Box::pin(async move {
                let debug_repl = get_controller();
                let action = args.trim();
                if action == "off" {
                    debug_repl.disable().await?;
                    ctx.ui()?
                        .notify(&"Debug REPL disabled".into(), Some(NotifyType::Info))?;
                    return Ok(());
                }
                if !action.is_empty() {
                    ctx.ui()?
                        .notify(&"Usage: /debug [off]".into(), Some(NotifyType::Warning))?;
                    return Ok(());
                }

                let session_id = ctx.session_manager()?.get_session_id().to_owned();
                debug_repl.enable(&session_id).await?;
                ctx.ui()?.notify(
                    &js_format!("Debug REPL enabled: pi debug attach {}", session_id),
                    Some(NotifyType::Warning),
                )?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
