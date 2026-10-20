//! Opt-in native `event-bus.ts`: explicit factories share the host's event bus.
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::extensions::*;
use pi_js_compat::string::js_trim;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Notification {
    message: String,
    from: String,
}

pub fn event_bus_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let current: Arc<Mutex<Option<ExtensionContext>>> = Arc::default();
    let session_context = current.clone();
    pi.on_session_start(
        move |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let current = session_context.clone();
            Box::pin(async move {
                *current.lock().map_err(|error| HandlerError::new(error.to_string()))? = Some(ctx);
                Ok(None)
            })
        },
    )?;
    pi.events.on(
        "my:notification",
        Arc::new(move |data| {
            let notification: Notification = serde_json::from_value(data.clone()).map_err(|error| error.to_string())?;
            let ctx = current.lock().map_err(|error| error.to_string())?.clone();
            if let Some(ctx) = ctx {
                ctx.ui().map_err(|error| error.to_string())?.notify(
                    &format!("Event from {}: {}", notification.from, notification.message),
                    Some(NotifyType::Info),
                );
            }
            Ok(())
        }),
    )?;
    let events = pi.events.clone();
    pi.register_command(
        "emit",
        Some("Emit my:notification event (usage: /emit message)".into()),
        None,
        Arc::new(move |args, _ctx| {
            let events = events.clone();
            Box::pin(async move {
                let message = js_trim(&args);
                let message = if message.is_empty() { "hello" } else { message };
                events.emit("my:notification", json!({"message":message,"from":"/emit command"}))?;
                Ok(())
            })
        }),
    )?;
    let events = pi.events.clone();
    pi.on_session_start(
        move |_event, _ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let events = events.clone();
            Box::pin(async move {
                events.emit(
                    "my:notification",
                    json!({"message":"Session started","from":"event-bus-example"}),
                )?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
