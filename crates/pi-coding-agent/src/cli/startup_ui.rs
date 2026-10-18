//! Startup UI helpers (`cli/startup-ui.ts`).
//!
//! Ported here: the official-distribution check, first-time-setup predicate,
//! theme loading, the startup TUI (`createStartupTui`, `startStartupTui`,
//! `applyDetectedStartupTheme`, `clearStartupTui`), and `showStartupSelector`.
//!
//! Blocked (documented in the traceability doc): `showStartupInput` and
//! `showFirstTimeSetup` need `ExtensionInputComponent` and
//! `FirstTimeSetupComponent`.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use pi_tui::terminal::Terminal;
use pi_tui::terminal_colors::RgbColor;
use pi_tui::tui::{ComponentRef, TuiExt, TuiMainScreen, component};

use crate::config;
use crate::core::keybindings::KeybindingsManager;
use crate::core::package_manager::{
    DefaultPackageManager, MissingSourceAction, ResolvedResource, SettingsManagerStore,
};
use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use crate::modes::interactive::components::extension_selector::{ExtensionSelectorComponent, ExtensionSelectorOptions};
use crate::modes::interactive::theme::{
    TerminalAutoThemeDetector, TerminalBackgroundThemeDetector, TerminalTheme, Theme, ThemeQueryError,
    detect_terminal_background_from_env, detect_terminal_theme_for_auto, init_theme, load_theme_from_path,
    parse_auto_theme_setting, resolve_theme_setting, set_registered_themes, set_theme,
};

const OFFICIAL_PACKAGE_NAME: &str = "@earendil-works/pi-coding-agent";
const OFFICIAL_APP_NAME: &str = "pi";
const OFFICIAL_CONFIG_DIR_NAME: &str = ".pi";

/// `isOfficialDistribution`.
pub fn is_official_distribution(package_name: &str, app_name: &str, config_dir_name: &str) -> bool {
    package_name == OFFICIAL_PACKAGE_NAME
        && app_name == OFFICIAL_APP_NAME
        && config_dir_name == OFFICIAL_CONFIG_DIR_NAME
}

fn distribution_is_official() -> bool {
    let config = config::package_config();
    is_official_distribution(&config.package_name, &config.app_name, &config.config_dir_name)
}

/// `shouldRunFirstTimeSetup`, with the environment passed in so the predicate
/// can be tested without mutating the process environment.
pub fn should_run_first_time_setup(settings_path: &Path, env: &BTreeMap<String, String>) -> bool {
    if !distribution_is_official() {
        return false;
    }
    let experimental = env.get("PI_EXPERIMENTAL").is_some_and(|value| value == "1");
    if !experimental {
        return false;
    }
    if env.contains_key(&config::env_agent_dir()) {
        return false;
    }
    !settings_path.exists()
}

/// `shouldRunFirstTimeSetup()` with the process environment.
pub fn should_run_first_time_setup_for_process() -> bool {
    should_run_first_time_setup(&config::get_settings_path(), &std::env::vars().collect())
}

/// `loadThemes`: enabled resources, broken themes skipped, names deduped.
pub fn load_themes(resources: &[ResolvedResource]) -> Vec<Arc<Theme>> {
    let mut themes = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for resource in resources {
        if !resource.enabled {
            continue;
        }
        let Ok(loaded_theme) = load_theme_from_path(Path::new(&resource.path), None) else {
            continue;
        };
        if let Some(name) = &loaded_theme.name
            && !seen.insert(name.clone())
        {
            continue;
        }
        themes.push(Arc::new(loaded_theme));
    }
    themes
}

/// `loadStartupThemes`: global settings only, missing packages skipped.
pub async fn load_startup_themes(settings_manager: &SettingsManager) -> Result<Vec<Arc<Theme>>, String> {
    let global_settings_manager = SettingsManager::in_memory(
        settings_manager.get_global_settings(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    );
    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_owned());
    let agent_dir = config::get_agent_dir().display().to_string();
    let package_manager = DefaultPackageManager::new(
        &cwd,
        &agent_dir,
        Arc::new(SettingsManagerStore::new(global_settings_manager)),
    );
    let resolved_paths = package_manager
        .resolve(Some(Box::new(|_source: String| {
            Box::pin(async { Ok(MissingSourceAction::Skip) })
        })))
        .await
        .map_err(|error| error.to_string())?;
    Ok(load_themes(&resolved_paths.themes))
}

const THEME_DETECTION_TIMEOUT: Duration = Duration::from_millis(100);
const CLEAR_DELAY: Duration = Duration::from_millis(25);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// `createStartupTui`.
pub async fn create_startup_tui(
    settings_manager: &SettingsManager,
    terminal: Rc<RefCell<dyn Terminal>>,
    env: &BTreeMap<String, String>,
) -> Result<Rc<TuiMainScreen>, String> {
    let themes = load_startup_themes(settings_manager).await?;
    set_registered_themes(themes).map_err(|error| error.to_string())?;
    let terminal_theme = detect_terminal_background_from_env(env).theme;
    let theme_name = resolve_theme_setting(settings_manager.get_theme_setting(), terminal_theme)
        .unwrap_or_else(|| terminal_theme.as_str().to_owned());
    init_theme(Some(&theme_name), false);
    let agent_dir = config::get_agent_dir();
    KeybindingsManager::create(Some(&agent_dir)).install();
    let ui = TuiMainScreen::with_options(
        terminal,
        Some(settings_manager.get_show_hardware_cursor()),
        Some(agent_dir),
    );
    ui.set_clear_on_shrink(settings_manager.get_clear_on_shrink());
    Ok(ui)
}

/// `startStartupTui`.
pub fn start_startup_tui(
    ui: &Rc<TuiMainScreen>,
    settings_manager: &SettingsManager,
    env: &BTreeMap<String, String>,
) -> Result<(), String> {
    ui.start().map_err(|error| error.to_string())?;
    apply_detected_startup_theme(ui, settings_manager, env);
    Ok(())
}

/// `applyDetectedStartupTheme`.
pub fn apply_detected_startup_theme(
    ui: &Rc<TuiMainScreen>,
    settings_manager: &SettingsManager,
    env: &BTreeMap<String, String>,
) {
    let theme_setting = settings_manager.get_theme_setting();
    if theme_setting.is_some() && parse_auto_theme_setting(theme_setting).is_none() {
        return;
    }
    let detector = StartupThemeDetector { ui: Rc::clone(ui) };
    let terminal_theme = detect_terminal_theme_for_auto(&detector, THEME_DETECTION_TIMEOUT, env);
    let name =
        resolve_theme_setting(theme_setting, terminal_theme).unwrap_or_else(|| terminal_theme.as_str().to_owned());
    set_theme(&name, false);
    ui.invalidate();
    ui.request_render(false);
}

/// `clearStartupTui`.
pub fn clear_startup_tui(ui: &TuiMainScreen) {
    ui.clear();
    ui.request_render(false);
    std::thread::sleep(CLEAR_DELAY);
}

/// `showStartupSelector`.
pub async fn show_startup_selector<T: Clone + 'static>(
    settings_manager: &SettingsManager,
    terminal: Rc<RefCell<dyn Terminal>>,
    env: &BTreeMap<String, String>,
    title: &str,
    options: Vec<(String, T)>,
) -> Result<Option<T>, String> {
    let ui = create_startup_tui(settings_manager, terminal, env).await?;
    let labels: Vec<String> = options.iter().map(|(label, _)| label.clone()).collect();
    let (sender, receiver) = channel::<Option<String>>();
    let select_sender = sender.clone();
    let cancel_sender = sender.clone();
    let request_render = {
        let ui = Rc::clone(&ui);
        Rc::new(move || ui.request_render(false)) as Rc<dyn Fn()>
    };
    let selector = ExtensionSelectorComponent::new(
        title,
        labels,
        Rc::new(move |selected: String| {
            let _ = select_sender.send(Some(selected));
        }),
        Rc::new(move || {
            let _ = cancel_sender.send(None);
        }),
        ExtensionSelectorOptions {
            request_render: Some(request_render),
            timeout: None,
            on_toggle_tools_expanded: None,
        },
    );
    let selector: ComponentRef = component(selector);
    ui.add_child(Rc::clone(&selector));
    ui.set_focus(Some(selector));
    start_startup_tui(&ui, settings_manager, env)?;
    let selected = wait_for_selection(&ui, &receiver);
    clear_startup_tui(&ui);
    ui.stop();
    Ok(selected.and_then(|selected| {
        options
            .into_iter()
            .find(|(label, _)| *label == selected)
            .map(|(_, value)| value)
    }))
}

fn wait_for_selection(ui: &TuiMainScreen, receiver: &Receiver<Option<String>>) -> Option<String> {
    loop {
        ui.process_events();
        ui.flush_pending();
        match receiver.try_recv() {
            Ok(selected) => return selected,
            Err(TryRecvError::Empty) => std::thread::sleep(POLL_INTERVAL),
            Err(TryRecvError::Disconnected) => return None,
        }
    }
}

struct StartupThemeDetector {
    ui: Rc<TuiMainScreen>,
}

impl TerminalBackgroundThemeDetector for StartupThemeDetector {
    fn query_terminal_background_color(&self, timeout: Duration) -> Result<Option<RgbColor>, ThemeQueryError> {
        let query = self.ui.query_terminal_background_color(timeout);
        wait_for_query(&self.ui, || query.result());
        Ok(query.result().flatten())
    }
}

impl TerminalAutoThemeDetector for StartupThemeDetector {
    fn query_terminal_color_scheme(&self, timeout: Duration) -> Option<Result<Option<TerminalTheme>, ThemeQueryError>> {
        let query = self.ui.query_terminal_color_scheme(timeout);
        wait_for_query(&self.ui, || query.result());
        query.result().map(|scheme| {
            Ok(scheme.map(|scheme| match scheme {
                pi_tui::terminal_colors::TerminalColorScheme::Dark => TerminalTheme::Dark,
                pi_tui::terminal_colors::TerminalColorScheme::Light => TerminalTheme::Light,
            }))
        })
    }
}

fn wait_for_query<T>(ui: &TuiMainScreen, result: impl Fn() -> Option<T>) {
    loop {
        ui.process_events();
        if result().is_some() {
            return;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
