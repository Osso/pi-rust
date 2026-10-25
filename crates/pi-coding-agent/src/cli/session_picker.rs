//! Session picker helpers (`cli/session-picker.ts`).
//!
//! Session loading/filtering/ordering and the startup selector event loop.

use pi_ai::request::CallbackError;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{TryRecvError, channel};
use std::time::Duration;

use pi_js_compat::js_json::JsString;
use pi_tui::terminal::Terminal;
use pi_tui::tui::{Component, ComponentRef, Focusable, TuiExt};

use crate::cli::startup_ui::{clear_startup_tui, create_startup_tui, start_startup_tui};
use crate::core::keybindings::KeybindingsManager;
use crate::core::resident_session::is_resident_session;
use crate::core::session_manager::SessionInfo;
use crate::core::settings_manager::SettingsManager;
use crate::modes::interactive::components::session_selector::{SessionListLoader, SessionSelectorComponent};

pub use crate::core::session_manager::SessionListProgress;

/// Future returned by [`SessionLoader::load`]; the TUI runs on a local set, so
/// it carries no `Send` bound.
pub type SessionListFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// `SessionsLoader`.
pub trait SessionLoader {
    fn load<'a>(
        &'a self,
        on_progress: Option<SessionListProgress<'a>>,
    ) -> SessionListFuture<'a, Result<Vec<SessionInfo>, JsString>>;
}

/// `loadSessions`: filter by archive flag and order named sessions first (for
/// the non-archived list) then by modification time descending.
pub async fn load_sessions<'a>(
    loader: &'a dyn SessionLoader,
    on_progress: Option<SessionListProgress<'a>>,
    archived: bool,
) -> Result<Vec<SessionInfo>, JsString> {
    let sessions = loader.load(on_progress).await?;
    let mut filtered_sessions: Vec<SessionInfo> = sessions
        .into_iter()
        .filter(|session| !is_resident_session(session))
        .filter(|session| session.is_archived.unwrap_or(false) == archived)
        .collect();
    pi_js_compat::array_sort::js_array_sort(&mut filtered_sessions, |left, right| {
        let left_named = is_named(left);
        let right_named = is_named(right);
        if !archived && left_named != right_named {
            return if left_named { -1.0 } else { 1.0 };
        }
        right.modified - left.modified
    });
    Ok(filtered_sessions)
}

fn is_named(session: &SessionInfo) -> bool {
    session.name.as_ref().is_some_and(|name| !name.trim().is_empty())
}

/// Outcome of [`select_session`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectSessionOutcome {
    /// The user selected a session path.
    Selected(JsString),
    /// The user cancelled (`No session selected`).
    Cancelled,
    /// `onExit`: the selector asked to exit the process with status 0.
    Exited,
}

/// Options for [`select_session`].
pub struct SelectSessionOptions<'a> {
    pub settings_manager: &'a SettingsManager,
    pub terminal: Rc<RefCell<dyn Terminal>>,
    pub env: &'a BTreeMap<String, String>,
    pub control_db_path: Option<&'a JsString>,
    pub archived_sessions_loader: Option<Arc<dyn SessionLoader>>,
    pub validate_selection: Option<ValidateSelection>,
}

/// `validateSelection`: throws to keep the selector open and show the error.
pub type ValidateSelection = Arc<dyn Fn(&JsString) -> Result<(), JsString>>;

/// Selector events consumed by the picker loop.
enum SelectEvent {
    Selected(JsString),
    Cancelled,
    Exited,
    ValidationError(JsString),
}

/// TUI child that lends the selector to the picker loop.
///
/// Async selector work is polled once per UI turn without blocking input.
struct SelectorHost {
    component: SessionSelectorComponent,
}

impl Component for SelectorHost {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.component.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.component.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.component.invalidate();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        self.component.as_focusable_mut()
    }

    fn as_container(&self) -> Option<&pi_tui::tui::Container> {
        self.component.as_container()
    }
}

/// `selectSession`: show the TUI session selector and return the selected path.
pub async fn select_session(
    current_sessions_loader: Arc<dyn SessionLoader>,
    all_sessions_loader: Arc<dyn SessionLoader>,
    options: SelectSessionOptions<'_>,
) -> Result<SelectSessionOutcome, CallbackError> {
    let ui = create_startup_tui(options.settings_manager, Rc::clone(&options.terminal), options.env).await?;
    let keybindings = KeybindingsManager::create(None).map_err(CallbackError::from)?;
    keybindings.clone().install();

    let (sender, receiver) = channel::<SelectEvent>();
    let resolved = Rc::new(RefCell::new(false));

    let on_select = {
        let sender = sender.clone();
        let resolved = Rc::clone(&resolved);
        let validate_selection = options.validate_selection.clone();
        Rc::new(move |path: JsString| {
            if *resolved.borrow() {
                return;
            }
            if let Some(validate) = &validate_selection
                && let Err(message) = validate(&path)
            {
                let _ = sender.send(SelectEvent::ValidationError(message));
                return;
            }
            *resolved.borrow_mut() = true;
            let _ = sender.send(SelectEvent::Selected(path));
        })
    };
    let on_cancel = {
        let sender = sender.clone();
        let resolved = Rc::clone(&resolved);
        Rc::new(move || {
            if *resolved.borrow() {
                return;
            }
            *resolved.borrow_mut() = true;
            let _ = sender.send(SelectEvent::Cancelled);
        })
    };
    let on_exit = {
        let sender = sender.clone();
        Rc::new(move || {
            let _ = sender.send(SelectEvent::Exited);
        })
    };
    let request_render = {
        let ui = Rc::clone(&ui);
        Rc::new(move || ui.request_render(false)) as Rc<dyn Fn()>
    };

    let control_db_path = options.control_db_path.cloned();
    let selector = SessionSelectorComponent::new(
        session_list_loader(current_sessions_loader, false),
        session_list_loader(all_sessions_loader, false),
        on_select,
        on_cancel,
        on_exit,
        request_render,
        None,
        Some(false),
        Some(keybindings),
        control_db_path.clone(),
        options
            .archived_sessions_loader
            .map(|loader| session_list_loader(loader, true)),
        None,
    );
    let list: ComponentRef = selector.session_list();
    let host = Rc::new(RefCell::new(SelectorHost { component: selector }));
    let host_ref: ComponentRef = Rc::clone(&host) as ComponentRef;
    ui.add_child(Rc::clone(&host_ref));
    ui.set_focus(Some(list));
    start_startup_tui(&ui, options.settings_manager, options.env)?;

    let outcome = loop {
        ui.process_events();
        host.borrow_mut().component.poll(std::time::Instant::now());
        ui.flush_pending()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        match receiver.try_recv() {
            Ok(SelectEvent::Selected(path)) => break SelectSessionOutcome::Selected(path),
            Ok(SelectEvent::Cancelled) => break SelectSessionOutcome::Cancelled,
            Ok(SelectEvent::Exited) => break SelectSessionOutcome::Exited,
            Ok(SelectEvent::ValidationError(message)) => {
                host.borrow_mut().component.show_error(&message);
                ui.request_render(false);
            }
            Err(TryRecvError::Empty) => std::thread::sleep(POLL_INTERVAL),
            Err(TryRecvError::Disconnected) => break SelectSessionOutcome::Cancelled,
        }
    };
    clear_startup_tui(&ui);
    ui.stop()?;
    Ok(outcome)
}

fn session_list_loader(loader: Arc<dyn SessionLoader>, archived: bool) -> SessionListLoader {
    Rc::new(move |on_progress| {
        let loader = Arc::clone(&loader);
        Box::pin(async move {
            let mut progress =
                on_progress.map(|callback| move |loaded: usize, total: usize| callback(loaded as f64, total as f64));
            let progress = progress.as_mut().map(|callback| callback as SessionListProgress<'_>);
            load_sessions(loader.as_ref(), progress, archived).await
        })
    })
}

const POLL_INTERVAL: Duration = Duration::from_millis(5);
