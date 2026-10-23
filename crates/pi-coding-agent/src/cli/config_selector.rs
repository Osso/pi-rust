//! TUI config selector for `pi config` (`cli/config-selector.ts`).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_tui::TUI;
use pi_tui::terminal::Terminal;
use pi_tui::tui::{ComponentRef, TuiExt};

use crate::core::package_manager::ResolvedPaths;
use crate::core::settings_manager::SettingsManager;
use crate::modes::interactive::components::config_selector::{
    ConfigSelectorComponent, ConfigSelectorOptions as ComponentOptions,
};
use crate::modes::interactive::theme::{init_theme, stop_theme_watcher};

/// `ConfigSelectorOptions`.
pub struct ConfigSelectorOptions {
    pub resolved_paths: ResolvedPaths,
    pub settings_manager: Rc<RefCell<SettingsManager>>,
    pub cwd: JsString,
    pub agent_dir: JsString,
}

/// How the selector closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectConfigOutcome {
    /// `onClose`: the selector closed normally.
    Closed,
    /// `onExit`: `process.exit(0)` — the caller should exit with status 0.
    Exit,
}

const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// `selectConfig`: show the TUI and return when it closes.
///
/// The TypeScript TUI owns its event loop; Rust's `TUI` is driven by
/// the caller, so this function polls events until a close/exit callback fires.
pub fn select_config(
    options: ConfigSelectorOptions,
    terminal: Rc<RefCell<dyn Terminal>>,
) -> Result<SelectConfigOutcome, pi_ai::request::CallbackError> {
    let theme = options.settings_manager.borrow().get_theme_setting().cloned();
    init_theme(theme.as_ref(), true).map_err(|error| error.message().clone())?;

    let agent_dir = options.agent_dir.clone();
    let ui = TUI::with_options(terminal, None, Some(PathBuf::from(encode_node_utf8(&agent_dir))));
    let (sender, receiver) = channel::<SelectConfigOutcome>();
    let close_sender = sender.clone();
    let exit_sender = sender.clone();
    let request_render = {
        let ui = Rc::clone(&ui);
        Rc::new(move || ui.request_render(false)) as Rc<dyn Fn()>
    };
    let terminal_height = ui.terminal().borrow().rows();

    let selector = ConfigSelectorComponent::new(ComponentOptions {
        resolved_paths: options.resolved_paths,
        settings_manager: Rc::clone(&options.settings_manager),
        cwd: options.cwd,
        agent_dir,
        on_close: Rc::new(move || {
            let _ = close_sender.send(SelectConfigOutcome::Closed);
        }),
        on_exit: Rc::new(move || {
            let _ = exit_sender.send(SelectConfigOutcome::Exit);
        }),
        request_render: Rc::clone(&request_render),
        terminal_height: Some(terminal_height),
    })?;
    let selector: ComponentRef = Rc::new(RefCell::new(selector));
    ui.add_child(Rc::clone(&selector));
    ui.set_focus(Some(Rc::clone(&selector)));
    ui.start().map_err(|error| JsString::from(error.to_string()))?;

    let outcome = wait_for_close(&ui, &receiver);
    ui.stop();
    stop_theme_watcher();
    Ok(outcome)
}

/// `selectConfig` wired to [`crate::package_manager_cli::ConfigSelector`].
///
/// The command-path settings store owns its manager behind a mutex, while the
/// config TUI needs `Rc<RefCell<SettingsManager>>` for its synchronous toggles.
/// The adapter reloads an equivalent manager for the TUI; every toggle writes
/// through to the same settings files and the command exits immediately after
/// the TUI closes, so the store's copy is not read again.
pub struct TuiConfigSelector {
    pub terminal: Rc<RefCell<dyn Terminal>>,
}

impl crate::package_manager_cli::ConfigSelector for TuiConfigSelector {
    fn select<'a>(
        &'a self,
        request: crate::package_manager_cli::SelectConfigRequest,
    ) -> crate::package_manager_cli::LocalBoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move {
            let settings_manager = SettingsManager::create(
                &request.cwd,
                Some(&request.agent_dir),
                crate::core::settings_manager::SettingsManagerCreateOptions {
                    project_trusted: Some(request.settings.with(|manager| manager.is_project_trusted())),
                },
            )
            .map_err(|error| error.message().clone())?;
            let outcome = select_config(
                ConfigSelectorOptions {
                    resolved_paths: request.resolved_paths,
                    settings_manager: Rc::new(RefCell::new(settings_manager)),
                    cwd: request.cwd,
                    agent_dir: request.agent_dir,
                },
                Rc::clone(&self.terminal),
            )
            .map_err(|error| error.message)?;
            // `onExit` calls `process.exit(0)`; the command ends with the same
            // status either way, so both outcomes complete normally here.
            let _ = outcome;
            Ok(())
        })
    }
}

fn wait_for_close(ui: &TUI, receiver: &Receiver<SelectConfigOutcome>) -> SelectConfigOutcome {
    loop {
        ui.process_events();
        ui.flush_pending();
        match receiver.try_recv() {
            Ok(outcome) => return outcome,
            Err(TryRecvError::Empty) => std::thread::sleep(POLL_INTERVAL),
            Err(TryRecvError::Disconnected) => return SelectConfigOutcome::Closed,
        }
    }
}
