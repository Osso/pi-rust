//! Startup UI helpers (`cli/startup-ui.ts`).
//!
//! Ported here: the official-distribution check, first-time-setup predicate,
//! theme loading, the startup TUI (`createStartupTui`, `startStartupTui`,
//! `applyDetectedStartupTheme`, `clearStartupTui`), and `showStartupSelector`.
//!
//! Also ported: `showStartupInput` and `showFirstTimeSetup`.

use pi_ai::request::CallbackError;
use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use pi_tui::TUI;
use pi_tui::terminal::Terminal;
use pi_tui::terminal_colors::RgbColor;
use pi_tui::tui::{ComponentRef, TuiExt, component};

use crate::modes::interactive::components::first_time_setup::{
    FirstTimeSetupComponent, FirstTimeSetupOptions, FirstTimeSetupResult,
};

use crate::config;
use crate::core::keybindings::KeybindingsManager;
use crate::core::package_manager::{
    DefaultPackageManager, MissingSourceAction, ResolvedResource, SettingsManagerStore,
};
use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use crate::modes::interactive::components::extension_input::{ExtensionInputComponent, ExtensionInputOptions};
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
pub fn is_official_distribution(
    package_name: impl Into<JsString>,
    app_name: impl Into<JsString>,
    config_dir_name: impl Into<JsString>,
) -> bool {
    let package_name = package_name.into();
    let app_name = app_name.into();
    let config_dir_name = config_dir_name.into();
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
    if env.get(&config::env_agent_dir()).is_some_and(|value| !value.is_empty()) {
        return false;
    }
    !settings_path.exists()
}

/// `shouldRunFirstTimeSetup()` with the process environment.
pub fn should_run_first_time_setup_for_process() -> Result<bool, config::ConfigError> {
    Ok(should_run_first_time_setup(
        std::path::Path::new(&pi_js_compat::js_json::encode_node_utf8(&config::get_settings_path()?)),
        &pi_js_compat::process_env::vars().into_iter().collect(),
    ))
}

/// `loadThemes`: enabled resources, broken themes skipped, names deduped.
pub fn load_themes(resources: &[ResolvedResource]) -> Vec<Arc<Theme>> {
    let mut themes = Vec::new();
    let mut seen: HashSet<JsString> = HashSet::new();
    for resource in resources {
        if !resource.enabled {
            continue;
        }
        let Ok(loaded_theme) = load_theme_from_path(&resource.path, None) else {
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
pub async fn load_startup_themes(settings_manager: &SettingsManager) -> Result<Vec<Arc<Theme>>, CallbackError> {
    let global_settings_manager = SettingsManager::in_memory(
        settings_manager.get_global_settings(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    );
    let cwd =
        pi_js_compat::process_cwd::cwd().map_err(|error| CallbackError::from_error(error.message.clone(), error))?;
    let agent_dir = config::get_agent_dir().map_err(CallbackError::from)?;
    let package_manager = DefaultPackageManager::new(
        &cwd,
        &agent_dir,
        Arc::new(SettingsManagerStore::new(global_settings_manager)),
    )
    .map_err(CallbackError::from)?;
    let resolved_paths = package_manager
        .resolve(Some(Box::new(|_source: JsString| {
            Box::pin(async { Ok(MissingSourceAction::Skip) })
        })))
        .await
        .map_err(CallbackError::from)?;
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
) -> Result<Rc<TUI>, CallbackError> {
    let themes = load_startup_themes(settings_manager).await?;
    set_registered_themes(themes).map_err(CallbackError::from)?;
    let terminal_theme = detect_terminal_background_from_env(env).theme;
    let theme_name = resolve_theme_setting(settings_manager.get_theme_setting(), terminal_theme)
        .unwrap_or_else(|| JsString::from(terminal_theme.as_str()));
    init_theme(Some(&theme_name), false).map_err(CallbackError::from)?;
    let agent_dir = config::get_agent_dir().map_err(CallbackError::from)?;
    KeybindingsManager::create(Some(&agent_dir))
        .map_err(CallbackError::from)?
        .install();
    let ui = TUI::with_options(
        terminal,
        Some(settings_manager.get_show_hardware_cursor()),
        Some(std::path::PathBuf::from(pi_js_compat::js_json::encode_node_utf8(
            &agent_dir,
        ))),
    );
    ui.set_clear_on_shrink(settings_manager.get_clear_on_shrink());
    Ok(ui)
}

/// `startStartupTui`.
pub fn start_startup_tui(
    ui: &Rc<TUI>,
    settings_manager: &SettingsManager,
    env: &BTreeMap<String, String>,
) -> Result<(), CallbackError> {
    ui.start()
        .map_err(|error| CallbackError::from_error(error.to_string().into(), error))?;
    apply_detected_startup_theme(ui, settings_manager, env);
    Ok(())
}

/// `applyDetectedStartupTheme`.
pub fn apply_detected_startup_theme(ui: &Rc<TUI>, settings_manager: &SettingsManager, env: &BTreeMap<String, String>) {
    let theme_setting = settings_manager.get_theme_setting();
    if theme_setting.is_some() && parse_auto_theme_setting(theme_setting).is_none() {
        return;
    }
    let detector = StartupThemeDetector { ui: Rc::clone(ui) };
    let terminal_theme = detect_terminal_theme_for_auto(&detector, THEME_DETECTION_TIMEOUT, env);
    let name =
        resolve_theme_setting(theme_setting, terminal_theme).unwrap_or_else(|| JsString::from(terminal_theme.as_str()));
    set_theme(&name, false)
        .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught(error.label(), error.to_string()));
    ui.invalidate();
    ui.request_render(false);
}

/// `showFirstTimeSetup`: theme choice and analytics opt-in, persisted on submit.
pub async fn show_first_time_setup(
    settings_manager: &mut SettingsManager,
    terminal: Rc<RefCell<dyn Terminal>>,
    env: &BTreeMap<String, String>,
) -> Result<(), CallbackError> {
    let ui = create_startup_tui(settings_manager, terminal, env).await?;
    let (sender, receiver) = channel::<Option<FirstTimeSetupResult>>();
    ui.start()
        .map_err(|error| CallbackError::from_error(error.to_string().into(), error))?;
    let detector = StartupThemeDetector { ui: Rc::clone(&ui) };
    let detected_theme = detect_terminal_theme_for_auto(&detector, THEME_DETECTION_TIMEOUT, env);
    set_theme(&detected_theme.as_str().into(), false).map_err(CallbackError::from)?;

    let submit_sender = sender.clone();
    let cancel_sender = sender;
    let preview_ui = Rc::clone(&ui);
    let setup = FirstTimeSetupComponent::new(FirstTimeSetupOptions {
        detected_theme,
        on_theme_preview: Rc::new(move |theme| {
            // The source ignores `setTheme`'s result in the preview callback.
            let _ = set_theme(&theme.as_str().into(), false);
            preview_ui.request_render(false);
        }),
        on_submit: Rc::new(move |result| {
            let _ = submit_sender.send(Some(result));
        }),
        on_cancel: Rc::new(move || {
            let _ = cancel_sender.send(None);
        }),
    });
    let component_ref: ComponentRef = component(setup);
    ui.add_child(Rc::clone(&component_ref));
    ui.set_focus(Some(component_ref));
    ui.request_render(false);

    let result = loop {
        ui.process_events();
        ui.flush_pending()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        match receiver.try_recv() {
            Ok(result) => break result,
            Err(TryRecvError::Empty) => std::thread::sleep(POLL_INTERVAL),
            Err(TryRecvError::Disconnected) => break None,
        }
    };
    if let Some(result) = result {
        settings_manager.set_theme(&result.theme.as_str().into());
        settings_manager.set_enable_analytics(result.share_analytics)?;
        settings_manager.flush();
    }
    clear_startup_tui(&ui);
    ui.stop()?;
    Ok(())
}

/// `clearStartupTui`.
pub fn clear_startup_tui(ui: &TUI) {
    ui.clear();
    ui.request_render(false);
    std::thread::sleep(CLEAR_DELAY);
}

/// `showStartupInput`.
pub async fn show_startup_input(
    settings_manager: &SettingsManager,
    terminal: Rc<RefCell<dyn Terminal>>,
    env: &BTreeMap<String, String>,
    title: &JsString,
    placeholder: Option<&JsString>,
) -> Result<Option<JsString>, CallbackError> {
    let ui = create_startup_tui(settings_manager, terminal, env).await?;
    let (sender, receiver) = channel::<Option<JsString>>();
    let submit_sender = sender.clone();
    let cancel_sender = sender;
    let input = ExtensionInputComponent::new(
        title,
        placeholder,
        Rc::new(move |value: JsString| {
            let _ = submit_sender.send(Some(value));
        }),
        Rc::new(move || {
            let _ = cancel_sender.send(None);
        }),
        ExtensionInputOptions::default(),
    );
    let input: ComponentRef = component(input);
    ui.add_child(Rc::clone(&input));
    ui.set_focus(Some(input));
    start_startup_tui(&ui, settings_manager, env)?;
    let value = wait_for_selection(&ui, &receiver);
    clear_startup_tui(&ui);
    ui.stop()?;
    Ok(value)
}

/// `showStartupSelector`.
pub async fn show_startup_selector<T: Clone + 'static>(
    settings_manager: &SettingsManager,
    terminal: Rc<RefCell<dyn Terminal>>,
    env: &BTreeMap<String, String>,
    title: &JsString,
    options: Vec<(JsString, T)>,
) -> Result<Option<T>, CallbackError> {
    let ui = create_startup_tui(settings_manager, terminal, env).await?;
    let labels: Vec<JsString> = options.iter().map(|(label, _)| label.clone()).collect();
    let (sender, receiver) = channel::<Option<JsString>>();
    let select_sender = sender.clone();
    let cancel_sender = sender.clone();
    let request_render = {
        let ui = Rc::clone(&ui);
        Rc::new(move || ui.request_render(false)) as Rc<dyn Fn()>
    };
    let selector = ExtensionSelectorComponent::new(
        title,
        labels,
        Rc::new(move |selected: JsString| {
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
    ui.stop()?;
    Ok(selected.and_then(|selected| {
        options
            .into_iter()
            .find(|(label, _)| *label == selected)
            .map(|(_, value)| value)
    }))
}

fn wait_for_selection(ui: &TUI, receiver: &Receiver<Option<JsString>>) -> Option<JsString> {
    loop {
        ui.process_events();
        ui.flush_pending()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        match receiver.try_recv() {
            Ok(selected) => return selected,
            Err(TryRecvError::Empty) => std::thread::sleep(POLL_INTERVAL),
            Err(TryRecvError::Disconnected) => return None,
        }
    }
}

struct StartupThemeDetector {
    ui: Rc<TUI>,
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

fn wait_for_query<T>(ui: &TUI, result: impl Fn() -> Option<T>) {
    loop {
        ui.process_events();
        if result().is_some() {
            return;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
