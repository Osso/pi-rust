//! Explicit native factory. Hosts supply WAD acquisition and the waived external
//! Doom runtime loader; no generated JavaScript/WASM interpreter is bundled here.
#[path = "doom_component.rs"]
pub mod doom_component;
#[path = "doom_engine.rs"]
pub mod doom_engine;
#[path = "doom_keys.rs"]
mod doom_keys;

use doom_component::DoomOverlayComponent;
use doom_engine::DoomEngine;
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

pub type EnsureWad = Arc<dyn Fn() -> BoxFuture<'static, Option<JsString>> + Send + Sync>;
pub type LoadEngine = Arc<dyn Fn(JsString) -> BoxFuture<'static, Result<DoomEngine, JsString>> + Send + Sync>;

#[derive(Default)]
struct Active {
    engine: Option<Arc<Mutex<DoomEngine>>>,
    wad: Option<JsString>,
}

pub fn doom_overlay_extension(
    api: &ExtensionApi,
    ensure_wad: EnsureWad,
    load_engine: LoadEngine,
) -> Result<(), HandlerError> {
    let active = Arc::new(Mutex::new(Active::default()));
    api.register_command(
        "doom-overlay",
        Some("Play DOOM as an overlay. Q to pause and exit.".into()),
        None,
        Arc::new(move |args, ctx| {
            let (active, ensure_wad, load_engine) = (active.clone(), ensure_wad.clone(), load_engine.clone());
            Box::pin(async move {
                if ctx.mode()? != ExtensionMode::Tui {
                    ctx.ui()?.notify(
                        &JsString::from("DOOM requires interactive mode"),
                        Some(NotifyType::Error),
                    )?;
                    return Ok(());
                }
                let ui = ctx.ui()?;
                ui.notify(&JsString::from("Loading DOOM..."), Some(NotifyType::Info))?;
                let args = args.trim();
                let wad = if args.is_empty() {
                    ensure_wad().await
                } else {
                    Some(args.to_owned())
                };
                let Some(wad) = wad.filter(|wad| !wad.is_empty()) else {
                    ui.notify(
                        &JsString::from("Failed to download DOOM WAD file. Check your internet connection."),
                        Some(NotifyType::Error),
                    )?;
                    return Ok(());
                };
                if let Err(error) = show_game(&*ui, &active, &load_engine, wad).await {
                    // Frozen doom-overlay/index.ts:40-78 uses String(error) in this catch.
                    let message = match error.thrown_value() {
                        pi_js_compat::abort::AbortReason::Error(error)
                        | pi_js_compat::abort::AbortReason::DomException(error) => error.string_value(),
                        pi_js_compat::abort::AbortReason::Value(_) => error.error_or_string_message()?,
                    };
                    ui.notify(&js_format!("Failed to load DOOM: {}", message), Some(NotifyType::Error))?;
                    *active.lock().unwrap() = Active::default();
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}

async fn show_game(
    ui: &dyn ExtensionUiContext,
    active: &Mutex<Active>,
    load_engine: &LoadEngine,
    wad: JsString,
) -> Result<(), pi_js_compat::callback_error::CallbackError> {
    let existing = {
        let active = active.lock().unwrap();
        active
            .engine
            .clone()
            .filter(|_| active.wad.as_ref() == Some(&JsString::from(&wad)))
    };
    let resume = existing.is_some();
    let engine = if let Some(engine) = existing {
        ui.notify(&JsString::from("Resuming DOOM..."), Some(NotifyType::Info))?;
        engine
    } else {
        ui.notify(&js_format!("Loading DOOM from {}...", wad), Some(NotifyType::Info))?;
        let engine = Arc::new(Mutex::new(load_engine(wad.clone()).await?));
        *active.lock().unwrap() = Active {
            engine: Some(engine.clone()),
            wad: Some(wad),
        };
        engine
    };
    ui.custom(
        Arc::new(move |context, done| {
            pi_coding_agent::core::extensions::ready_custom_component(Box::new(DoomOverlayComponent::new(
                context.tui,
                engine.clone(),
                done,
                resume,
            )?))
        }),
        Some(ExtensionUiCustomOptions {
            overlay: Some(true),
            overlay_options: Some(Arc::new(|| ExtensionUiOverlayOptions {
                width: Some(pi_tui::tui::SizeValue::Percent(75.0)),
                max_height: Some(pi_tui::tui::SizeValue::Percent(95.0)),
                anchor: Some(pi_tui::tui::OverlayAnchor::Center),
                margin: Some(pi_tui::tui::OverlayMargin {
                    top: Some(1),
                    ..Default::default()
                }),
                ..Default::default()
            })),
            ..Default::default()
        }),
    )
    .await?;
    Ok(())
}
