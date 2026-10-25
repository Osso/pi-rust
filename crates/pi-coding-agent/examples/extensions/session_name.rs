//! Opt-in native `session-name.ts`; never loaded by default.
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_coding_agent::core::extensions::{ExtensionApi, HandlerError, NotifyType};

pub fn session_name_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.register_command(
        "session-name",
        Some("Set or show session name (usage: /session-name [new name])".into()),
        None,
        Arc::new(move |args, ctx| {
            let api = api.clone();
            Box::pin(async move {
                let name = args.trim();
                let message = if !name.is_empty() {
                    api.set_session_name(&name)?;
                    js_format!("Session named: {}", name)
                } else {
                    match api.get_session_name()?.filter(|name| !name.is_empty()) {
                        Some(current) => js_format!("Session: {}", current),
                        None => "No session name set".into(),
                    }
                };
                ctx.ui()?.notify(&JsString::from(&message), Some(NotifyType::Info))?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
