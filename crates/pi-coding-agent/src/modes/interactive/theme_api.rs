//! Narrow theme surface consumed by the interactive components.
//!
//! `modes/interactive/theme/theme.ts` owns the concrete `Theme`; components hold
//! an `Rc<dyn ThemeApi>` so they can render against any implementation. The
//! builder functions mirror `getSelectListTheme` / `getSettingsListTheme` in
//! `theme.ts` with the global theme replaced by the explicit handle.

use std::rc::Rc;

use pi_tui::{SelectListTheme, SettingsListTheme};

/// Foreground colors used by the ported components (`ThemeColor` subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColor {
    Accent,
    Border,
    BorderAccent,
    CustomMessageLabel,
    CustomMessageText,
    Dim,
    Error,
    MdLink,
    Muted,
    Success,
    Text,
    Warning,
}

/// Background colors used by the ported components (`ThemeBg` subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeBg {
    CustomMessageBg,
    SelectedBg,
}

/// Styling surface the interactive components require from the active theme.
pub trait ThemeApi {
    fn fg(&self, color: ThemeColor, text: &str) -> String;

    fn bg(&self, color: ThemeBg, text: &str) -> String;

    fn bold(&self, text: &str) -> String;

    fn italic(&self, text: &str) -> String;
}

/// `getSelectListTheme()` with the global theme replaced by `theme`.
pub fn select_list_theme(theme: &Rc<dyn ThemeApi>) -> SelectListTheme {
    SelectListTheme {
        selected_prefix: color_style(theme, ThemeColor::Accent),
        selected_text: color_style(theme, ThemeColor::Accent),
        description: color_style(theme, ThemeColor::Muted),
        scroll_info: color_style(theme, ThemeColor::Muted),
        no_match: color_style(theme, ThemeColor::Muted),
    }
}

/// `getSettingsListTheme()` with the global theme replaced by `theme`.
pub fn settings_list_theme(theme: &Rc<dyn ThemeApi>) -> SettingsListTheme {
    let accent_label = theme.clone();
    let accent_value = theme.clone();
    let muted = theme.clone();
    let dim = theme.clone();
    let dim_hint = theme.clone();
    SettingsListTheme {
        label: Rc::new(move |text: &str, selected: bool| {
            if selected {
                accent_label.fg(ThemeColor::Accent, text)
            } else {
                text.to_string()
            }
        }),
        value: Rc::new(move |text: &str, selected: bool| {
            if selected {
                accent_value.fg(ThemeColor::Accent, text)
            } else {
                muted.fg(ThemeColor::Muted, text)
            }
        }),
        description: Rc::new(move |text: &str| dim.fg(ThemeColor::Dim, text)),
        cursor: theme.fg(ThemeColor::Accent, "→ "),
        hint: Rc::new(move |text: &str| dim_hint.fg(ThemeColor::Dim, text)),
    }
}

/// `(text) => theme.fg(color, text)` bound to a shared theme handle.
pub fn color_style(theme: &Rc<dyn ThemeApi>, color: ThemeColor) -> Rc<dyn Fn(&str) -> String> {
    let theme = theme.clone();
    Rc::new(move |text: &str| theme.fg(color, text))
}

/// `(text) => theme.bg(color, text)` bound to a shared theme handle.
pub fn background_style(theme: &Rc<dyn ThemeApi>, color: ThemeBg) -> Rc<dyn Fn(&str) -> String> {
    let theme = theme.clone();
    Rc::new(move |text: &str| theme.bg(color, text))
}
