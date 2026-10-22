//! Opt-in native port of `input-transform.ts`; register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::date;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

pub fn input_transform_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    register_with_clock(pi, Arc::new(|| date::js_now_millis() as i64))
}

// Clock seam is local to this embeddable example, not a new extension API.
pub(crate) fn register_with_clock(
    pi: &ExtensionApi,
    now: Arc<dyn Fn() -> i64 + Send + Sync>,
) -> Result<(), HandlerError> {
    pi.on_input(
        move |event: InputEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<InputEventResult>, HandlerError>> {
            let now = now.clone();
            Box::pin(async move {
                if event.source == InputSource::Extension {
                    return Ok(Some(InputEventResult::Continue));
                }
                if let Some(query) = event.text.strip_prefix("?quick ") {
                    let query = query.trim();
                    if query.is_empty() {
                        ctx.ui()?
                            .notify(&JsString::from("Usage: ?quick <question>"), Some(NotifyType::Warning));
                        return Ok(Some(InputEventResult::Handled));
                    }
                    return Ok(Some(InputEventResult::Transform {
                        text: pi_js_compat::js_format!("Respond briefly in 1-2 sentences: {}", query),
                        images: None,
                    }));
                }
                // Only ASCII input can lowercase to either ASCII keyword in JS.
                if event.text.to_lowercase() == "ping" {
                    ctx.ui()?.notify(&JsString::from("pong"), Some(NotifyType::Info));
                    return Ok(Some(InputEventResult::Handled));
                }
                if event.text.to_lowercase() == "time" {
                    let time = date::to_locale_string(now()).ok_or_else(|| {
                        HandlerError::new(
                            "cannot format local date: unavailable time zone or unsupported calendar range",
                        )
                    })?;
                    ctx.ui()?.notify(&JsString::from(&time), Some(NotifyType::Info));
                    return Ok(Some(InputEventResult::Handled));
                }
                Ok(Some(InputEventResult::Continue))
            })
        },
    )
    .map_err(HandlerError::new)
}
