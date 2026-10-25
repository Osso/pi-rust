//! Ported from `coding-agent/src/modes/interactive/components/first-time-setup.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::rc::Rc;

use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, Container, component};

use super::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::{key_hint, raw_key_hint};
use crate::modes::interactive::theme::{TerminalTheme, ThemeColor, with_theme};

/// `FirstTimeSetupResult`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirstTimeSetupResult {
    pub theme: TerminalTheme,
    pub share_analytics: bool,
}

/// `FirstTimeSetupOptions`.
pub struct FirstTimeSetupOptions {
    pub detected_theme: TerminalTheme,
    pub on_theme_preview: Rc<dyn Fn(TerminalTheme)>,
    pub on_submit: Rc<dyn Fn(FirstTimeSetupResult)>,
    pub on_cancel: Rc<dyn Fn()>,
}

const THEME_OPTIONS: [(TerminalTheme, &str); 2] = [(TerminalTheme::Dark, "Dark"), (TerminalTheme::Light, "Light")];
const ANALYTICS_OPTIONS: [(bool, &str); 2] = [(true, "Share anonymous usage data"), (false, "Don't share")];
const SETUP_LOGO_LINES: [&str; 4] = ["██████", "██  ██", "████  ██", "██    ██"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SetupStep {
    Theme,
    Analytics,
}

/// First-time setup dialog: theme choice and analytics opt-in.
pub struct FirstTimeSetupComponent {
    step: SetupStep,
    theme_index: usize,
    analytics_index: usize,
    options: FirstTimeSetupOptions,
    container: Container,
}

impl FirstTimeSetupComponent {
    pub fn new(options: FirstTimeSetupOptions) -> Self {
        let theme_index = THEME_OPTIONS
            .iter()
            .position(|(value, _)| *value == options.detected_theme)
            .unwrap_or(0);
        let mut component = Self {
            step: SetupStep::Theme,
            theme_index,
            analytics_index: 0,
            options,
            container: Container::new(),
        };
        component.update();
        component
    }

    fn update(&mut self) {
        self.container.clear();
        self.container.add_child(component(dynamic_border()));
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Accent, SETUP_LOGO_LINES.join("\n"))),
            1,
            0,
        )));
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(component(Text::new(
            with_theme(|theme| {
                theme.fg(
                    ThemeColor::Accent,
                    theme.bold(&js_format!(
                        "Welcome to {}, the minimal coding agent.",
                        crate::config::package_config().app_name
                    )),
                )
            }),
            1,
            0,
        )));
        self.container.add_child(component(Spacer::new(1)));

        match self.step {
            SetupStep::Theme => {
                self.container.add_child(component(Text::new(
                    with_theme(|theme| theme.fg(ThemeColor::Text, "Pick a theme.")),
                    1,
                    0,
                )));
                self.container.add_child(component(Text::new(
                    with_theme(|theme| {
                        theme.fg(
                            ThemeColor::Muted,
                            &js_format!("Detected system appearance: {}", self.options.detected_theme.as_str()),
                        )
                    }),
                    1,
                    0,
                )));
                self.container.add_child(component(Spacer::new(1)));
                let labels: Vec<&str> = THEME_OPTIONS.iter().map(|(_, label)| *label).collect();
                self.add_option_list(&labels, self.theme_index);
            }
            SetupStep::Analytics => {
                self.container.add_child(component(Text::new(
                    with_theme(|theme| theme.fg(ThemeColor::Text, "Opt-in to anonymous usage data sharing?")),
                    1,
                    0,
                )));
                self.container.add_child(component(Text::new(
                    with_theme(|theme| {
                        theme.fg(
                            ThemeColor::Muted,
                            "Opting in stores a tracking identifier in settings.json and enables anonymous\nusage analytics. This helps us to better debug, reproduce, and resolve issues\nand bugs within Pi. You can observe what is shared using /privacy and make\nchanges anytime in settings.json.",
                        )
                    }),
                    1,
                    0,
                )));
                self.container.add_child(component(Spacer::new(1)));
                let labels: Vec<&str> = ANALYTICS_OPTIONS.iter().map(|(_, label)| *label).collect();
                self.add_option_list(&labels, self.analytics_index);
            }
        }

        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(component(Text::new(
            raw_key_hint("↑↓", "navigate")
                + "  "
                + &key_hint(
                    "tui.select.confirm",
                    if self.step == SetupStep::Theme {
                        "continue"
                    } else {
                        "finish"
                    },
                )
                + "  "
                + &key_hint("tui.select.cancel", "skip setup"),
            1,
            0,
        )));
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(component(dynamic_border()));
    }

    fn add_option_list(&mut self, labels: &[&str], selected_index: usize) {
        for (index, label) in labels.iter().enumerate() {
            let is_selected = index == selected_index;
            let text = with_theme(|theme| {
                let prefix = if is_selected {
                    theme.fg(ThemeColor::Accent, "→ ")
                } else {
                    JsString::from("  ")
                };
                let label = if is_selected {
                    theme.fg(ThemeColor::Accent, *label)
                } else {
                    theme.fg(ThemeColor::Text, *label)
                };
                js_format!("{}{}", prefix, label)
            });
            self.container.add_child(component(Text::new(text, 1, 0)));
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.step == SetupStep::Theme {
            let next = self
                .theme_index
                .saturating_add_signed(delta)
                .min(THEME_OPTIONS.len() - 1);
            if next != self.theme_index {
                self.theme_index = next;
                (self.options.on_theme_preview)(THEME_OPTIONS[self.theme_index].0);
            }
        } else {
            self.analytics_index = self
                .analytics_index
                .saturating_add_signed(delta)
                .min(ANALYTICS_OPTIONS.len() - 1);
        }
        self.update();
    }
}

impl Component for FirstTimeSetupComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.up") || data == "k" {
            self.move_selection(-1);
        } else if keybindings.matches(data, "tui.select.down") || data == "j" {
            self.move_selection(1);
        } else if keybindings.matches(data, "tui.select.confirm") || data == "\n" {
            if self.step == SetupStep::Theme {
                self.step = SetupStep::Analytics;
                self.update();
            } else {
                (self.options.on_submit)(FirstTimeSetupResult {
                    theme: THEME_OPTIONS[self.theme_index].0,
                    share_analytics: ANALYTICS_OPTIONS[self.analytics_index].0,
                });
            }
        } else if keybindings.matches(data, "tui.select.cancel") {
            (self.options.on_cancel)();
        }
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}
