//! Opt-in native `trigger-compact.ts`; compacts only on upward threshold crossings.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};
const COMPACT_THRESHOLD_TOKENS: f64 = 100_000.0;

pub fn trigger_compact_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let previous = Arc::new(Mutex::new(None::<f64>));
    pi.on_turn_end(
        move |_: TurnEndEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let previous = previous.clone();
            Box::pin(async move {
                let Some(tokens) = ctx.get_context_usage()?.and_then(|usage| usage.tokens) else {
                    return Ok(None);
                };
                let crossed = {
                    let mut previous = previous.lock().map_err(|e| HandlerError::new(e.to_string()))?;
                    let crossed = previous.is_some_and(|tokens| tokens <= COMPACT_THRESHOLD_TOKENS);
                    *previous = Some(tokens);
                    crossed
                };
                if crossed && tokens > COMPACT_THRESHOLD_TOKENS {
                    trigger_compaction(ctx, None)?;
                }
                Ok(None)
            })
        },
    )?;
    pi.register_command(
        "trigger-compact",
        Some("Trigger compaction immediately".into()),
        None,
        Arc::new(|args, ctx| {
            Box::pin(async move {
                let instructions = args.trim();
                let instructions = (!instructions.is_empty()).then(|| instructions.to_owned());
                trigger_compaction(ctx.context, instructions)
            })
        }),
    )
    .map_err(HandlerError::new)
}

fn trigger_compaction(ctx: ExtensionContext, instructions: Option<JsString>) -> Result<(), HandlerError> {
    let has_ui = ctx.has_ui()?;
    if has_ui {
        ctx.ui()?
            .notify(&JsString::from("Compaction started"), Some(NotifyType::Info))?;
    }
    let complete_ui = ctx.ui()?;
    let error_ui = complete_ui.clone();
    ctx.compact(Some(CompactOptions {
        custom_instructions: instructions,
        on_complete: Some(Arc::new(move |_| {
            if has_ui {
                complete_ui.notify(&JsString::from("Compaction completed"), Some(NotifyType::Info))?;
            }
            Ok(())
        })),
        on_error: Some(Arc::new(move |error| {
            if has_ui {
                error_ui.notify(
                    &js_format!("Compaction failed: {}", error.message),
                    Some(NotifyType::Error),
                )?;
            }
            Ok(())
        })),
    }))
}
