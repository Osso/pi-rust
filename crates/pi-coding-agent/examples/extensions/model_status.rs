//! Opt-in native `model-status.ts`. Default output is stdout, like console.log.
use pi_js_compat::js_json::JsString;
use std::io::{Write, stdout};
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::extensions::*;

pub fn model_status_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    model_status_with_output(pi, stdout())
}

/// Embedding hosts may supply their console writer without changing core APIs.
pub fn model_status_with_output(pi: &ExtensionApi, output: impl Write + Send + 'static) -> Result<(), HandlerError> {
    let output = Arc::new(Mutex::new(output));
    pi.on_model_select(
        move |event: ModelSelectEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let output = output.clone();
            Box::pin(async move {
                let next = format!("{}/{}", event.model.provider, event.model.id);
                let previous = event
                    .previous_model
                    .map_or_else(|| "none".into(), |model| format!("{}/{}", model.provider, model.id));
                let ui = ctx.ui()?;
                if event.source != ModelSelectSource::Restore {
                    ui.notify(&JsString::from(format!("Model: {next}")), Some(NotifyType::Info));
                }
                ui.set_status(
                    &JsString::from("model"),
                    Some(&JsString::from(format!("🤖 {}", event.model.id))),
                );
                let source = match event.source {
                    ModelSelectSource::Set => "set",
                    ModelSelectSource::Cycle => "cycle",
                    ModelSelectSource::Restore => "restore",
                    ModelSelectSource::Fallback => "fallback",
                };
                let line = format!("[model_select] {previous} → {next} ({source})\n");
                output
                    .lock()
                    .map_err(|error| HandlerError::new(error.to_string()))?
                    .write_all(line.as_bytes())
                    .map_err(|error| HandlerError::new(error.to_string()))?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
