//! Session picker helpers (`cli/session-picker.ts`).
//!
//! `selectSession` needs `SessionSelectorComponent` and `cli/startup-ui.ts`,
//! both still owned by the interactive slices. The session loading/filtering/
//! naming/ordering helper is ported here and is the whole data path behind the
//! selector.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{TryRecvError, channel};
use std::time::Duration;

use pi_tui::terminal::Terminal;
use pi_tui::tui::{Component, ComponentRef, Focusable, TuiExt};

use crate::cli::startup_ui::{clear_startup_tui, create_startup_tui, start_startup_tui};
use crate::core::keybindings::KeybindingsManager;
use crate::core::session_control_db::list_named_sessions;
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
    ) -> SessionListFuture<'a, Result<Vec<SessionInfo>, String>>;
}

/// `loadSessionsWithControlNames`: filter by archive flag, merge control-db
/// names, and order named sessions first (for the non-archived list) then by
/// modification time descending.
pub async fn load_sessions_with_control_names<'a>(
    loader: &'a dyn SessionLoader,
    control_db_path: Option<&'a str>,
    on_progress: Option<SessionListProgress<'a>>,
    archived: bool,
) -> Result<Vec<SessionInfo>, String> {
    let sessions = loader.load(on_progress).await?;
    let mut filtered_sessions: Vec<SessionInfo> = sessions
        .into_iter()
        .filter(|session| session.is_archived.unwrap_or(false) == archived)
        .collect();
    let Some(control_db_path) = control_db_path else {
        return Ok(filtered_sessions);
    };

    let names = named_sessions_by_path(control_db_path)?;
    for session in &mut filtered_sessions {
        if let Some(name) = names.get(&session.path) {
            session.name = Some(name.clone());
        }
    }
    filtered_sessions.sort_by(|left, right| {
        let left_named = names.contains_key(&left.path);
        let right_named = names.contains_key(&right.path);
        if !archived && left_named != right_named {
            return if left_named {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        right.modified.cmp(&left.modified)
    });
    Ok(filtered_sessions)
}

fn named_sessions_by_path(control_db_path: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let mut names = std::collections::HashMap::new();
    for session in list_named_sessions(control_db_path).map_err(|error| error.to_string())? {
        names.insert(session.session_path, session.name);
    }
    Ok(names)
}

/// Outcome of [`select_session`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectSessionOutcome {
    /// The user selected a session path.
    Selected(String),
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
    pub control_db_path: Option<&'a str>,
    pub archived_sessions_loader: Option<Arc<dyn SessionLoader>>,
    pub validate_selection: Option<ValidateSelection>,
}

/// `validateSelection`: throws to keep the selector open and show the error.
pub type ValidateSelection = Arc<dyn Fn(&str) -> Result<(), String>>;

/// Selector events consumed by the picker loop.
enum SelectEvent {
    Selected(String),
    Cancelled,
    Exited,
    ValidationError(String),
}

/// TUI child that lends the selector to the picker loop.
///
/// The loop takes the component out while it awaits async work, so no
/// `RefCell` borrow is ever held across an await point.
struct SelectorHost {
    component: Option<SessionSelectorComponent>,
}

impl Component for SelectorHost {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.component
            .as_mut()
            .map(|component| component.render(width))
            .unwrap_or_default()
    }

    fn handle_input(&mut self, data: &str) {
        if let Some(component) = self.component.as_mut() {
            component.handle_input(data);
        }
    }

    fn invalidate(&mut self) {
        if let Some(component) = self.component.as_mut() {
            component.invalidate();
        }
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        self.component
            .as_mut()
            .and_then(|component| component.as_focusable_mut())
    }
}

/// `selectSession`: show the TUI session selector and return the selected path.
pub async fn select_session(
    current_sessions_loader: Arc<dyn SessionLoader>,
    all_sessions_loader: Arc<dyn SessionLoader>,
    options: SelectSessionOptions<'_>,
) -> Result<SelectSessionOutcome, String> {
    let ui = create_startup_tui(options.settings_manager, Rc::clone(&options.terminal), options.env).await?;
    let keybindings = KeybindingsManager::create(Some(&crate::config::get_agent_dir()));
    keybindings.clone().install();

    let (sender, receiver) = channel::<SelectEvent>();
    let resolved = Rc::new(RefCell::new(false));

    let on_select = {
        let sender = sender.clone();
        let resolved = Rc::clone(&resolved);
        let validate_selection = options.validate_selection.clone();
        Rc::new(move |path: String| {
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

    let control_db_path = options.control_db_path.map(str::to_owned);
    let mut selector = SessionSelectorComponent::new(
        session_list_loader(current_sessions_loader, control_db_path.clone(), false),
        session_list_loader(all_sessions_loader, control_db_path.clone(), false),
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
            .map(|loader| session_list_loader(loader, control_db_path, true)),
        None,
    );
    selector.load_current_sessions().await;

    let host = Rc::new(RefCell::new(SelectorHost {
        component: Some(selector),
    }));
    let host_ref: ComponentRef = Rc::clone(&host) as ComponentRef;
    ui.add_child(Rc::clone(&host_ref));
    ui.set_focus(Some(host_ref));
    start_startup_tui(&ui, options.settings_manager, options.env)?;

    let outcome = loop {
        ui.process_events();
        ui.flush_pending();
        let pending = host
            .borrow()
            .component
            .as_ref()
            .is_some_and(SessionSelectorComponent::has_pending_action);
        if pending {
            let Some(mut selector) = host.borrow_mut().component.take() else {
                continue;
            };
            selector.run_pending_action().await;
            host.borrow_mut().component = Some(selector);
        }
        match receiver.try_recv() {
            Ok(SelectEvent::Selected(path)) => break SelectSessionOutcome::Selected(path),
            Ok(SelectEvent::Cancelled) => break SelectSessionOutcome::Cancelled,
            Ok(SelectEvent::Exited) => break SelectSessionOutcome::Exited,
            Ok(SelectEvent::ValidationError(message)) => {
                if let Some(selector) = host.borrow_mut().component.as_mut() {
                    selector.show_error(&message);
                }
                ui.request_render(false);
            }
            Err(TryRecvError::Empty) => std::thread::sleep(POLL_INTERVAL),
            Err(TryRecvError::Disconnected) => break SelectSessionOutcome::Cancelled,
        }
    };
    clear_startup_tui(&ui);
    ui.stop();
    Ok(outcome)
}

fn session_list_loader(
    loader: Arc<dyn SessionLoader>,
    control_db_path: Option<String>,
    archived: bool,
) -> SessionListLoader {
    Rc::new(move |on_progress| {
        let loader = Arc::clone(&loader);
        let control_db_path = control_db_path.clone();
        Box::pin(async move {
            let mut progress = on_progress.map(|callback| move |loaded, total| callback(loaded, total));
            let progress = progress.as_mut().map(|callback| callback as SessionListProgress<'_>);
            load_sessions_with_control_names(loader.as_ref(), control_db_path.as_deref(), progress, archived).await
        })
    })
}

const POLL_INTERVAL: Duration = Duration::from_millis(5);
