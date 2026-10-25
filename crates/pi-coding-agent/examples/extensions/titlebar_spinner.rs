//! Opt-in native `titlebar-spinner.ts`; embed and register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
type Timer = Arc<Mutex<Option<JoinHandle<()>>>>;

fn read_base_title(pi: &ExtensionApi) -> Result<JsString, HandlerError> {
    let cwd = pi_js_compat::process_cwd::cwd().map_err(HandlerError::from)?;
    let basename = pi_coding_agent::utils::paths::node_basename(&cwd);
    Ok(match pi.get_session_name()?.filter(|name| !name.is_empty()) {
        Some(session) => js_format!("π - {} - {}", session, basename),
        None => js_format!("π - {}", basename),
    })
}
fn stop_animation(timer: &Timer, pi: &ExtensionApi, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    if let Some(task) = timer.lock().unwrap().take() {
        task.abort();
    }
    ctx.ui()?.set_title(&read_base_title(pi)?)?;
    Ok(())
}
fn start_animation(timer: &Timer, pi: &ExtensionApi, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    stop_animation(timer, pi, ctx)?;
    let api = pi.clone();
    let ui = ctx.ui()?;
    let delay = Duration::from_millis(80);
    let first = tokio::time::Instant::now() + delay;
    let task = tokio::spawn(async move {
        let mut ticks = tokio::time::interval_at(first, delay);
        let mut frame = 0;
        loop {
            ticks.tick().await;
            match read_base_title(&api) {
                Ok(base) => {
                    if let Err(error) = ui.set_title(&js_format!("{} {}", FRAMES[frame % FRAMES.len()], base)) {
                        // Frozen titlebar-spinner.ts:37-43 has no catch around the interval callback.
                        // Actual-source process probe: uuid_source_boundaries.
                        pi_js_compat::callback_error::throw_callback_uncaught(error);
                    }
                }
                Err(error) => {
                    eprintln!("titlebar-spinner: {error}");
                    return;
                }
            }
            frame += 1;
        }
    });
    *timer.lock().unwrap() = Some(task);
    Ok(())
}
pub fn titlebar_spinner_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let timer: Timer = Arc::default();
    let current = timer.clone();
    let api = pi.clone();
    pi.on_agent_start(move |_event, ctx: ExtensionContext| {
        let result = start_animation(&current, &api, &ctx);
        Box::pin(async move { result.map(|()| None) }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    let current = timer.clone();
    let api = pi.clone();
    pi.on_agent_end(move |_event, ctx: ExtensionContext| {
        let result = stop_animation(&current, &api, &ctx);
        Box::pin(async move { result.map(|()| None) }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    let api = pi.clone();
    pi.on_session_shutdown(move |_event, ctx: ExtensionContext| {
        let result = stop_animation(&timer, &api, &ctx);
        Box::pin(async move { result.map(|()| None) }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })
    .map_err(HandlerError::new)
}
