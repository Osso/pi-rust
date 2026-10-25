//! Opt-in native `working-indicator.ts`; embed and register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::ThemeColor;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum Mode {
    Dot,
    None,
    Pulse,
    Spinner,
    Default,
}
impl Mode {
    fn description(self) -> &'static str {
        match self {
            Self::Dot => "static dot",
            Self::None => "hidden",
            Self::Pulse => "custom pulse",
            Self::Spinner => "custom spinner",
            Self::Default => "pi default spinner",
        }
    }
    fn indicator(self) -> Option<WorkingIndicatorOptions> {
        let colors = [
            "255;179;186",
            "255;223;186",
            "255;255;186",
            "186;255;201",
            "186;225;255",
            "218;186;255",
        ];
        let (frames, interval_ms): (Vec<(&str, usize)>, _) = match self {
            Self::Dot => (vec![("●", 0)], None),
            Self::None => (vec![], None),
            Self::Pulse => (vec![("·", 0), ("•", 2), ("●", 4), ("•", 5)], Some(120.0)),
            Self::Spinner => (
                ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                    .into_iter()
                    .enumerate()
                    .map(|(i, frame)| (frame, i % colors.len()))
                    .collect(),
                Some(80.0),
            ),
            Self::Default => return None,
        };
        Some(WorkingIndicatorOptions {
            frames: Some(
                frames
                    .into_iter()
                    .map(|(text, index)| format!("\x1b[38;2;{}m{text}\x1b[39m", colors[index]).into())
                    .collect(),
            ),
            interval_ms,
        })
    }
}
fn apply_indicator(mode: Mode, ui: &dyn ExtensionUiContext) -> Result<(), HandlerError> {
    ui.set_working_indicator(mode.indicator().as_ref());
    ui.set_status(
        &JsString::from("working-indicator"),
        Some(ExtensionStatus::Text(
            ui.theme()?
                .fg(ThemeColor::Dim, &format!("Indicator: {}", mode.description())),
        )),
    )
    .map_err(HandlerError::from)
}
pub fn working_indicator_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let mode = Arc::new(Mutex::new(Mode::Spinner));
    let current = mode.clone();
    pi.on_session_start(move |_event, ctx: ExtensionContext| {
        let mode = *current.lock().unwrap();
        Box::pin(async move {
            apply_indicator(mode, ctx.ui()?.as_ref())?;
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    pi.register_command(
        "working-indicator",
        Some("Set the streaming working indicator: dot, pulse, none, spinner, or reset.".into()),
        None,
        Arc::new(move |args, ctx| {
            let current = mode.clone();
            Box::pin(async move {
                let ui = ctx.ui()?;
                let next = (&args).trim().to_lowercase();
                if next.is_empty() {
                    ui.notify(
                        &JsString::from(format!("Working indicator: {}", current.lock().unwrap().description())),
                        Some(NotifyType::Info),
                    )?;
                    return Ok(());
                }
                let mode = match &next {
                    mode if mode == "dot" => Mode::Dot,
                    mode if mode == "none" => Mode::None,
                    mode if mode == "pulse" => Mode::Pulse,
                    mode if mode == "spinner" => Mode::Spinner,
                    mode if mode == "reset" => Mode::Default,
                    _ => {
                        ui.notify(
                            &JsString::from("Usage: /working-indicator [dot|pulse|none|spinner|reset]"),
                            Some(NotifyType::Error),
                        )?;
                        return Ok(());
                    }
                };
                *current.lock().unwrap() = mode;
                apply_indicator(mode, ui.as_ref())?;
                ui.notify(
                    &JsString::from(format!("Working indicator set to: {}", mode.description())),
                    Some(NotifyType::Info),
                )?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
