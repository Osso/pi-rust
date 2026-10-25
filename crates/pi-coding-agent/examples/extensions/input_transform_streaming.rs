//! Opt-in native port of `input-transform-streaming.ts`; register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_regex::JsRegex as Regex;
use pi_js_compat::string::js_trim;
use std::sync::Arc;

pub fn input_transform_streaming_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    let trigger = Arc::new(Regex::with_flags(r"\b(changes?|diff|modified)\b", "i").expect("constant JS pattern"));
    pi.on_input(
        move |event: InputEvent,
              _ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<InputEventResult>, HandlerError>> {
            let api = api.clone();
            let trigger = trigger.clone();
            Box::pin(async move {
                if event.streaming_behavior == Some(StreamingBehavior::Steer) || !trigger.is_match(&event.text) {
                    return Ok(Some(InputEventResult::Continue));
                }
                let result = api
                    .exec("git".into(), vec!["diff".into(), "--stat".into()], None)
                    .await?;
                let stat = js_trim(&result.stdout);
                if result.code != 0 || stat.is_empty() {
                    return Ok(Some(InputEventResult::Continue));
                }
                Ok(Some(InputEventResult::Transform {
                    text: js_format!("{}\n\nCurrent uncommitted changes:\n```\n{}\n```", event.text, stat),
                    images: None,
                }))
            })
        },
    )
    .map_err(HandlerError::new)
}
