//! Ported from `packages/coding-agent/src/modes/interactive/theme/theme-controller.ts`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::terminal_colors::RgbColor;

use super::{
    TerminalTheme, Theme, ThemeError, ThemeQueryError, detect_terminal_background_from_env,
    detect_terminal_background_theme, detect_terminal_theme_for_auto, init_theme, parse_auto_theme_setting,
    resolve_theme_setting, set_theme, set_theme_instance,
};
use crate::core::settings_manager::{SharedSettingsManager, lock_settings_manager};
use crate::utils::uncaught::throw_uncaught;

/// The TUI surface used by the theme controller.
pub trait ThemeUi {
    fn invalidate(&self);
    fn request_render(&self);
    fn set_terminal_color_scheme_notifications(&self, enabled: bool);
    fn on_terminal_color_scheme_change(&self, listener: Rc<dyn Fn(TerminalTheme)>) -> usize;
    fn query_terminal_background_color(&self, timeout: Duration) -> Result<Option<RgbColor>, ThemeQueryError>;
    /// Optional `queryTerminalColorScheme`; `None` means the TUI lacks it.
    fn query_terminal_color_scheme(
        &self,
        _timeout: Duration,
    ) -> Option<Result<Option<TerminalTheme>, ThemeQueryError>> {
        None
    }
}

struct BackgroundDetector(Rc<dyn ThemeUi>);

impl super::TerminalBackgroundThemeDetector for BackgroundDetector {
    fn query_terminal_background_color(&self, timeout: Duration) -> Result<Option<RgbColor>, ThemeQueryError> {
        self.0.query_terminal_background_color(timeout)
    }
}

impl super::TerminalAutoThemeDetector for BackgroundDetector {
    fn query_terminal_color_scheme(&self, timeout: Duration) -> Option<Result<Option<TerminalTheme>, ThemeQueryError>> {
        self.0.query_terminal_color_scheme(timeout)
    }
}

struct ControllerCore {
    ui: Rc<dyn ThemeUi>,
    settings_manager: SharedSettingsManager,
    show_error: Rc<dyn Fn(&JsString)>,
    on_changed: Rc<dyn Fn()>,
    terminal_theme: TerminalTheme,
    active_theme_name: Option<JsString>,
    auto_sync_enabled: bool,
}

impl ControllerCore {
    fn notify_changed(&self) {
        self.ui.invalidate();
        (self.on_changed)();
    }

    fn set_auto_sync(&mut self, enabled: bool) {
        if self.auto_sync_enabled == enabled {
            return;
        }
        self.auto_sync_enabled = enabled;
        self.ui.set_terminal_color_scheme_notifications(enabled);
    }

    fn apply_theme_name(&mut self, theme_name: &JsString, show_error: bool) -> super::SetThemeResult {
        let result = set_theme(theme_name, true).unwrap_or_else(|error| throw_uncaught(error.label(), error.message()));
        self.active_theme_name = Some(if result.success {
            theme_name.to_owned()
        } else {
            "dark".into()
        });
        self.notify_changed();
        if !result.success
            && show_error
            && let Some(error) = &result.error
        {
            (self.show_error)(&js_format!(
                "Failed to load theme \"{}\": {}\nFell back to dark theme.",
                theme_name,
                error
            ));
        }
        result
    }

    fn apply_terminal_theme(&mut self, terminal_theme: TerminalTheme) {
        if !self.auto_sync_enabled {
            return;
        }
        self.terminal_theme = terminal_theme;
        let setting = lock_settings_manager(&self.settings_manager)
            .get_theme_setting()
            .map(JsString::from);
        let auto_theme = parse_auto_theme_setting(setting.as_ref());
        let Some(auto_theme) = auto_theme else {
            self.set_auto_sync(false);
            return;
        };
        let theme_name = match terminal_theme {
            TerminalTheme::Light => auto_theme.light_theme,
            TerminalTheme::Dark => auto_theme.dark_theme,
        };
        if Some(&theme_name) != self.active_theme_name.as_ref() {
            self.apply_theme_name(&theme_name, false);
        }
    }
}

/// Settings-driven theme selection with terminal light/dark auto-sync.
pub struct InteractiveThemeController {
    core: Rc<RefCell<ControllerCore>>,
}

impl InteractiveThemeController {
    pub fn new(
        ui: Rc<dyn ThemeUi>,
        settings_manager: SharedSettingsManager,
        show_error: Rc<dyn Fn(&JsString)>,
        on_changed: Rc<dyn Fn()>,
    ) -> Result<Self, ThemeError> {
        let terminal_theme = detect_terminal_background_from_env(&process_env()).theme;
        let active_theme_name = resolve_theme_setting(
            lock_settings_manager(&settings_manager)
                .get_theme_setting()
                .map(JsString::from)
                .as_ref(),
            terminal_theme,
        );
        init_theme(active_theme_name.as_ref(), true)?;
        let core = Rc::new(RefCell::new(ControllerCore {
            ui,
            settings_manager,
            show_error,
            on_changed,
            terminal_theme,
            active_theme_name,
            auto_sync_enabled: false,
        }));
        let controller = Self { core };
        controller.bind_terminal_color_scheme_listener();
        Ok(controller)
    }

    pub fn apply_from_settings(&mut self) {
        let theme_setting = lock_settings_manager(&self.core.borrow().settings_manager)
            .get_theme_setting()
            .map(JsString::from);
        let env = process_env();
        let ui = Rc::clone(&self.core.borrow().ui);
        if let Some(auto_theme) = parse_auto_theme_setting(theme_setting.as_ref()) {
            let detector = BackgroundDetector(ui);
            let terminal_theme = detect_terminal_theme_for_auto(&detector, Duration::from_millis(100), &env);
            {
                let mut core = self.core.borrow_mut();
                core.set_auto_sync(true);
                core.terminal_theme = terminal_theme;
            }
            let theme_name = match terminal_theme {
                TerminalTheme::Light => auto_theme.light_theme,
                TerminalTheme::Dark => auto_theme.dark_theme,
            };
            self.core.borrow_mut().apply_theme_name(&theme_name, true);
            return;
        }

        self.core.borrow_mut().set_auto_sync(false);
        if let Some(theme_setting) = theme_setting {
            self.core.borrow_mut().apply_theme_name(&theme_setting, true);
            return;
        }

        let detector = BackgroundDetector(ui);
        let detection = detect_terminal_background_theme(&detector, Duration::from_millis(100), &env);
        {
            let mut core = self.core.borrow_mut();
            core.terminal_theme = detection.theme;
            if !core.apply_theme_name(&detection.theme.as_str().into(), false).success {
                return;
            }
        }
        if detection.confidence == super::ThemeDetectionConfidence::High {
            let settings = Arc::clone(&self.core.borrow().settings_manager);
            let mut settings = lock_settings_manager(&settings);
            settings.set_theme(&detection.theme.as_str().into());
            settings.flush();
        }
    }

    pub fn set_theme_name(&mut self, theme_name: &JsString, show_error: bool) -> super::SetThemeResult {
        let mut core = self.core.borrow_mut();
        core.set_auto_sync(false);
        core.apply_theme_name(theme_name, show_error)
    }

    pub fn set_theme_instance(&mut self, theme_instance: Arc<Theme>) -> super::SetThemeResult {
        {
            let mut core = self.core.borrow_mut();
            core.set_auto_sync(false);
            set_theme_instance(theme_instance);
            core.active_theme_name = Some("<in-memory>".into());
            core.notify_changed();
        }
        super::SetThemeResult {
            success: true,
            error: None,
        }
    }

    pub fn preview(&mut self, theme_setting_or_name: &JsString) {
        let (terminal_theme, active_theme_name) = {
            let core = self.core.borrow();
            (core.terminal_theme, core.active_theme_name.clone())
        };
        let theme_name = resolve_theme_setting(Some(theme_setting_or_name), terminal_theme).or(active_theme_name);
        let Some(theme_name) = theme_name else {
            return;
        };
        if set_theme(&theme_name, true)
            .unwrap_or_else(|error| throw_uncaught(error.label(), error.message()))
            .success
        {
            let ui = Rc::clone(&self.core.borrow().ui);
            ui.invalidate();
            ui.request_render();
        }
    }

    pub fn disable_auto_sync(&mut self) {
        self.core.borrow_mut().set_auto_sync(false);
    }

    pub fn get_terminal_theme(&self) -> TerminalTheme {
        self.core.borrow().terminal_theme
    }

    fn bind_terminal_color_scheme_listener(&self) {
        let core = Rc::clone(&self.core);
        let ui = Rc::clone(&self.core.borrow().ui);
        let listener = Rc::new(move |terminal_theme: TerminalTheme| {
            core.borrow_mut().apply_terminal_theme(terminal_theme);
        });
        ui.on_terminal_color_scheme_change(listener);
    }
}

fn process_env() -> BTreeMap<String, String> {
    pi_js_compat::process_env::vars().into_iter().collect()
}
