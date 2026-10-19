//! Behavioral tests for `modes/interactive/theme/theme-controller.ts`.
//!
//! Each case runs in a probe child so `PI_PACKAGE_DIR` can point at fixture
//! themes and the process-global theme state starts clean.

#[path = "support/env_probe.rs"]
mod env_probe;

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

fn fixture_package_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("package fixture dir");
    fs::create_dir(directory.path().join("theme")).expect("theme dir");
    for name in ["dark.json", "light.json"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("theme").join(name),
            directory.path().join("theme").join(name),
        )
        .expect("copy built-in theme");
    }
    directory
}

fn fixture_agent_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("agent fixture dir");
    fs::create_dir(directory.path().join("themes")).expect("themes dir");
    directory
}

fn run_controller_probe(probe: &str, package_dir: &Path, agent_dir: &Path) {
    let environment: Vec<(&str, OsString)> = vec![
        ("PI_PACKAGE_DIR", package_dir.as_os_str().to_owned()),
        ("PI_CODING_AGENT_DIR", agent_dir.as_os_str().to_owned()),
    ];
    env_probe::run_probe(probe, &environment);
}

#[test]
fn applies_explicit_and_preview_themes() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    run_controller_probe("theme_controller_explicit_probe", package.path(), agent.path());
}

#[test]
#[ignore = "child process probe"]
fn theme_controller_explicit_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let (mut controller, ui, changed, _settings) = build_controller(Some("light"));
    use pi_coding_agent::modes::interactive::theme::current_theme;

    assert_eq!(current_theme().name.as_deref(), Some("light"));

    let result = controller.set_theme_name("dark", false);
    assert!(result.success);
    assert_eq!(result.error, None);
    assert_eq!(current_theme().name.as_deref(), Some("dark"));
    assert_eq!(ui.invalidations.get(), 1);
    assert_eq!(changed.get(), 1);
    assert!(ui.notifications.borrow().is_empty());

    controller.preview("light");
    assert_eq!(current_theme().name.as_deref(), Some("light"));
    assert_eq!(ui.render_requests.get(), 1);
    assert_eq!(ui.invalidations.get(), 2);

    let failure = controller.set_theme_name("missing-theme", true);
    assert!(!failure.success);
    assert_eq!(failure.error.as_deref(), Some("Theme not found: missing-theme"));
    assert_eq!(current_theme().name.as_deref(), Some("dark"));
    let errors = ui.errors.borrow();
    assert_eq!(
        errors.last().map(String::as_str),
        Some("Failed to load theme \"missing-theme\": Theme not found: missing-theme\nFell back to dark theme.")
    );
}

#[test]
fn auto_sync_follows_terminal_color_scheme() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    run_controller_probe("theme_controller_auto_probe", package.path(), agent.path());
}

#[test]
#[ignore = "child process probe"]
fn theme_controller_auto_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::modes::interactive::theme::{TerminalTheme, current_theme};
    use pi_tui::terminal_colors::RgbColor;

    let (mut controller, ui, _changed, _settings) = build_controller(Some("light/dark"));
    ui.background.set(Some(Ok(Some(RgbColor { r: 250, g: 250, b: 250 }))));
    controller.apply_from_settings();
    assert_eq!(current_theme().name.as_deref(), Some("light"));
    assert_eq!(ui.notifications.borrow().last(), Some(&true));
    assert_eq!(controller.get_terminal_theme(), TerminalTheme::Light);

    ui.emit_color_scheme(TerminalTheme::Dark);
    assert_eq!(current_theme().name.as_deref(), Some("dark"));
    assert_eq!(controller.get_terminal_theme(), TerminalTheme::Dark);

    controller.disable_auto_sync();
    assert_eq!(ui.notifications.borrow().last(), Some(&false));
    ui.emit_color_scheme(TerminalTheme::Light);
    assert_eq!(current_theme().name.as_deref(), Some("dark"));
}

#[test]
fn detects_and_persists_terminal_theme() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    run_controller_probe("theme_controller_detect_probe", package.path(), agent.path());
}

#[test]
#[ignore = "child process probe"]
fn theme_controller_detect_probe() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::modes::interactive::theme::current_theme;
    use pi_tui::terminal_colors::RgbColor;

    let (mut controller, ui, _changed, settings) = build_controller(None);
    ui.background.set(Some(Ok(Some(RgbColor { r: 8, g: 8, b: 8 }))));
    controller.apply_from_settings();
    assert_eq!(current_theme().name.as_deref(), Some("dark"));
    assert_eq!(settings.borrow().get_theme_setting(), Some("dark"));

    ui.background.set(Some(Ok(None)));
    controller.apply_from_settings();
    assert_eq!(current_theme().name.as_deref(), Some("dark"));
}

#[test]
fn rebinds_tui_listeners() {
    let package = fixture_package_dir();
    let agent = fixture_agent_dir();
    run_controller_probe("theme_controller_rebind_probe", package.path(), agent.path());
}

#[test]
#[ignore = "child process probe"]
fn theme_controller_rebind_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let (mut controller, ui, _changed, _settings) = build_controller(Some("dark"));
    assert_eq!(ui.listener_count(), 1);
    controller.rebind_tui();
    assert_eq!(ui.listener_count(), 1);
    assert_eq!(ui.removed.get(), 1);
}

// ============================================================================
// Fake TUI + settings
// ============================================================================

type SchemeListener = std::rc::Rc<dyn Fn(pi_coding_agent::modes::interactive::theme::TerminalTheme)>;
type BackgroundResult =
    Result<Option<pi_tui::terminal_colors::RgbColor>, pi_coding_agent::modes::interactive::theme::ThemeQueryError>;

#[derive(Default)]
struct FakeUi {
    invalidations: std::cell::Cell<usize>,
    render_requests: std::cell::Cell<usize>,
    removed: std::cell::Cell<usize>,
    notifications: std::cell::RefCell<Vec<bool>>,
    listeners: std::cell::RefCell<Vec<(usize, SchemeListener)>>,
    next_id: std::cell::Cell<usize>,
    background: std::cell::Cell<Option<BackgroundResult>>,
    errors: std::cell::RefCell<Vec<String>>,
}

impl FakeUi {
    fn emit_color_scheme(&self, theme: pi_coding_agent::modes::interactive::theme::TerminalTheme) {
        let listeners = self.listeners.borrow().clone();
        for (_, listener) in listeners {
            listener(theme);
        }
    }

    fn listener_count(&self) -> usize {
        self.listeners.borrow().len()
    }
}

impl pi_coding_agent::modes::interactive::theme::ThemeUi for FakeUi {
    fn invalidate(&self) {
        self.invalidations.set(self.invalidations.get() + 1);
    }

    fn request_render(&self) {
        self.render_requests.set(self.render_requests.get() + 1);
    }

    fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
        self.notifications.borrow_mut().push(enabled);
    }

    fn on_terminal_color_scheme_change(&self, listener: SchemeListener) -> usize {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        self.listeners.borrow_mut().push((id, listener));
        id
    }

    fn remove_terminal_color_scheme_listener(&self, id: usize) {
        self.removed.set(self.removed.get() + 1);
        self.listeners
            .borrow_mut()
            .retain(|(listener_id, _)| *listener_id != id);
    }

    fn query_terminal_background_color(
        &self,
        _timeout: std::time::Duration,
    ) -> Result<Option<pi_tui::terminal_colors::RgbColor>, pi_coding_agent::modes::interactive::theme::ThemeQueryError>
    {
        match self.background.take() {
            Some(result) => {
                let response = result.clone();
                self.background.set(Some(result));
                response
            }
            None => Ok(None),
        }
    }
}

type ControllerParts = (
    pi_coding_agent::modes::interactive::theme::InteractiveThemeController,
    std::rc::Rc<FakeUi>,
    std::rc::Rc<std::cell::Cell<usize>>,
    std::rc::Rc<std::cell::RefCell<pi_coding_agent::core::settings_manager::SettingsManager>>,
);

fn build_controller(theme_setting: Option<&str>) -> ControllerParts {
    use std::rc::Rc;

    use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
    use pi_coding_agent::modes::interactive::theme::InteractiveThemeController;

    let agent_dir = std::env::var("PI_CODING_AGENT_DIR").expect("PI_CODING_AGENT_DIR");
    let cwd = std::env::temp_dir().display().to_string();
    let mut settings = SettingsManager::create(
        &cwd,
        Some(std::path::Path::new(&agent_dir)),
        SettingsManagerCreateOptions { project_trusted: None },
    )
    .expect("settings manager");
    if let Some(theme_setting) = theme_setting {
        settings.set_theme(theme_setting);
    }
    let settings = Rc::new(std::cell::RefCell::new(settings));

    let ui = Rc::new(FakeUi::default());
    let ui_handle: Rc<dyn pi_coding_agent::modes::interactive::theme::ThemeUi> = ui.clone();
    let changed = Rc::new(std::cell::Cell::new(0));
    let changed_sink = Rc::clone(&changed);
    let error_ui = Rc::clone(&ui);
    let controller = InteractiveThemeController::new(
        ui_handle,
        Rc::clone(&settings),
        Rc::new(move |message: &str| error_ui.errors.borrow_mut().push(message.to_owned())),
        Rc::new(move || changed_sink.set(changed_sink.get() + 1)),
    )
    .expect("theme controller");
    (controller, ui, changed, settings)
}
