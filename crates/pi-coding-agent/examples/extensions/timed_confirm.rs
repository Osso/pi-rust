//! Opt-in native `timed-confirm.ts`; embed and register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::{sync::Arc, time::Duration};

pub fn timed_confirm_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "timed",
        Some("Show a timed confirmation dialog (auto-cancels in 5s with countdown)".into()),
        None,
        Arc::new(|_args, ctx| {
            Box::pin(async move {
                let ui = ctx.ui()?;
                let options = ExtensionUiDialogOptions {
                    timeout: Some(5000.0),
                    ..Default::default()
                };
                let confirmed = ui
                    .confirm(
                        &JsString::from("Timed Confirmation"),
                        &JsString::from("This dialog will auto-cancel in 5 seconds. Confirm?"),
                        Some(&options),
                    )
                    .await?;
                ui.notify(
                    &JsString::from(if confirmed {
                        "Confirmed by user!"
                    } else {
                        "Cancelled or timed out"
                    }),
                    Some(NotifyType::Info),
                )?;
                Ok(())
            })
        }),
    )?;
    pi.register_command(
        "timed-select",
        Some("Show a timed select dialog (auto-cancels in 10s with countdown)".into()),
        None,
        Arc::new(|_args, ctx| {
            Box::pin(async move {
                let ui = ctx.ui()?;
                let options = ExtensionUiDialogOptions {
                    timeout: Some(10000.0),
                    ..Default::default()
                };
                let choice = ui
                    .select(
                        &JsString::from("Pick an option"),
                        &["Option A".into(), "Option B".into(), "Option C".into()],
                        Some(&options),
                    )
                    .await?;
                let message = match choice.filter(|choice| !choice.is_empty()) {
                    Some(choice) => js_format!("Selected: {}", choice),
                    None => "Selection cancelled or timed out".into(),
                };
                ui.notify(&JsString::from(&message), Some(NotifyType::Info))?;
                Ok(())
            })
        }),
    )?;
    pi.register_command(
        "timed-signal",
        Some("Show a timed confirm using AbortSignal (manual approach)".into()),
        None,
        Arc::new(|_args, ctx| Box::pin(confirm_with_signal(ctx))),
    )
    .map_err(HandlerError::new)
}

async fn confirm_with_signal(ctx: ExtensionCommandContext) -> Result<(), HandlerError> {
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let controller_abort = controller_signal.clone();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(5000);
    let timer = tokio::spawn(async move {
        tokio::time::sleep_until(deadline).await;
        controller_abort.abort(None);
    });
    let ui = ctx.ui()?;
    ui.notify(
        &JsString::from("Dialog will auto-cancel in 5 seconds..."),
        Some(NotifyType::Info),
    )?;
    let options = ExtensionUiDialogOptions {
        signal: Some(signal.clone()),
        ..Default::default()
    };
    let confirmed = ui
        .confirm(
            &JsString::from("Timed Confirmation"),
            &JsString::from("This dialog will auto-cancel in 5 seconds. Confirm?"),
            Some(&options),
        )
        .await?;
    timer.abort();
    let (message, level) = if confirmed {
        ("Confirmed by user!", NotifyType::Info)
    } else if signal.aborted() {
        ("Dialog timed out (auto-cancelled)", NotifyType::Warning)
    } else {
        ("Cancelled by user", NotifyType::Info)
    };
    ui.notify(&JsString::from(message), Some(level))?;
    Ok(())
}
