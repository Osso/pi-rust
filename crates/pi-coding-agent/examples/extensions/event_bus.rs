//! Opt-in native `event-bus.ts`: explicit factories share the host's event bus.
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use serde::Deserialize;

#[derive(Deserialize)]
struct Notification {
    message: JsString,
    from: JsString,
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
        &JsString::from("my:notification"),
        Arc::new(move |data| {
            let notification: Notification =
                pi_js_compat::js_json::from_value(data).map_err(|error| JsString::from(error.to_string()))?;
            let ctx = current
                .lock()
                .map_err(|error| JsString::from(error.to_string()))?
                .clone();
            if let Some(ctx) = ctx {
                ctx.ui().map_err(|error| error.message)?.notify(
                    &js_format!("Event from {}: {}", notification.from, notification.message),
                    Some(NotifyType::Info),
                )?;
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
                let message = args.trim();
                let message = if message.is_empty() {
                    JsString::from("hello")
                } else {
                    message
                };
                events.emit(
                    &JsString::from("my:notification"),
                    json!({"message":message,"from":"/emit command"}),
                )?;
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
                    &JsString::from("my:notification"),
                    json!({"message":"Session started","from":"event-bus-example"}),
                )?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
