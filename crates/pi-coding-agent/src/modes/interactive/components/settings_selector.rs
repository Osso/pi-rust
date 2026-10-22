//! Ported from `coding-agent/src/modes/interactive/components/settings-selector.ts`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::settings_manager::ScrollViewScrollbar;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::types::Transport;
use pi_js_compat::{js_format, js_json::JsString, number::parse_int};
use pi_tui::components::settings_list::{SettingItem, SettingsList, SettingsListOptions, SubmenuDone};
use pi_tui::tui::TuiMode;
use pi_tui::tui::component;
use pi_tui::{Component, SelectItem, SelectList, SelectListLayoutOptions, Spacer, Text};

use crate::core::http_dispatcher::{HTTP_IDLE_TIMEOUT_CHOICES, format_http_idle_timeout_ms};
use crate::core::settings_manager::{
    DefaultProjectTrust, FullscreenExitOutput, MermaidRenderingMode, TreeFilterMode, WarningSettings,
};
use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::key_display_text;
use crate::modes::interactive::theme::{
    TerminalTheme, ThemeColor, get_select_list_theme, get_settings_list_theme, parse_auto_theme_setting,
};

const SETTINGS_SUBMENU_SELECT_LIST_LAYOUT: SelectListLayoutOptions = SelectListLayoutOptions {
    min_primary_column_width: Some(12),
    max_primary_column_width: Some(32),
    truncate_primary: None,
};

fn thinking_description(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Off => "No reasoning",
        ThinkingLevel::Minimal => "Very brief reasoning (~1k tokens)",
        ThinkingLevel::Low => "Light reasoning (~2k tokens)",
        ThinkingLevel::Medium => "Moderate reasoning (~8k tokens)",
        ThinkingLevel::High => "Deep reasoning (~16k tokens)",
        ThinkingLevel::XHigh => "Extra-high reasoning (~32k tokens)",
        ThinkingLevel::Max => "Maximum reasoning depth",
        ThinkingLevel::Ultra => "Maximum reasoning with automatic delegation",
    }
}

fn default_project_trust_label(value: DefaultProjectTrust) -> &'static str {
    match value {
        DefaultProjectTrust::Ask => "Ask",
        DefaultProjectTrust::Always => "Always trust",
        DefaultProjectTrust::Never => "Never trust",
    }
}

fn default_project_trust_from_label(label: &JsString) -> Option<DefaultProjectTrust> {
    let Ok(label) = label.to_utf8() else {
        return None;
    };
    match label.as_str() {
        "Ask" => Some(DefaultProjectTrust::Ask),
        "Always trust" => Some(DefaultProjectTrust::Always),
        "Never trust" => Some(DefaultProjectTrust::Never),
        _ => None,
    }
}

fn default_project_trust_labels() -> Vec<JsString> {
    [
        DefaultProjectTrust::Ask,
        DefaultProjectTrust::Always,
        DefaultProjectTrust::Never,
    ]
    .into_iter()
    .map(|value| default_project_trust_label(value).into())
    .collect()
}

/// Steering queue mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteeringMode {
    All,
    OneAtATime,
}

impl SteeringMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::OneAtATime => "one-at-a-time",
        }
    }

    fn parse(value: &JsString) -> Option<Self> {
        let Ok(value) = value.to_utf8() else {
            return None;
        };
        match value.as_str() {
            "all" => Some(Self::All),
            "one-at-a-time" => Some(Self::OneAtATime),
            _ => None,
        }
    }
}

/// Follow-up queue mode.
pub type FollowUpMode = SteeringMode;

/// `onThemePreview`.
pub type ThemePreviewFn = Rc<dyn Fn(&JsString)>;

/// Submenu selection-change callback.
pub type SelectionChangeFn = Rc<dyn Fn(&JsString)>;

/// Theme submenu completion callback (`done(selectedValue?)`).
pub type ThemeDoneFn = Rc<dyn Fn(Option<&JsString>)>;

/// SettingsList change callback.
pub type SettingsChangeHandler = Rc<dyn Fn(&JsString, &JsString)>;

/// Settings shown by the selector.
pub struct SettingsConfig {
    pub auto_compact: bool,
    pub show_images: bool,
    pub image_width_cells: i64,
    pub auto_resize_images: bool,
    pub block_images: bool,
    pub enable_skill_commands: bool,
    pub steering_mode: SteeringMode,
    pub follow_up_mode: FollowUpMode,
    pub transport: Transport,
    pub http_idle_timeout_ms: u64,
    pub thinking_level: ThinkingLevel,
    pub available_thinking_levels: Vec<ThinkingLevel>,
    pub current_theme: JsString,
    pub terminal_theme: TerminalTheme,
    pub available_themes: Vec<JsString>,
    pub hide_thinking_block: bool,
    pub mermaid_rendering_mode: MermaidRenderingMode,
    pub show_cache_miss_notices: bool,
    pub collapse_changelog: bool,
    pub enable_install_telemetry: bool,
    pub double_escape_action: crate::core::settings_manager::DoubleEscapeAction,
    pub tree_filter_mode: TreeFilterMode,
    pub show_hardware_cursor: bool,
    pub editor_padding_x: i64,
    pub output_pad: u8,
    pub autocomplete_max_visible: i64,
    pub quiet_startup: bool,
    pub default_project_trust: DefaultProjectTrust,
    pub clear_on_shrink: bool,
    pub show_terminal_progress: bool,
    pub tui_mode: TuiMode,
    pub fullscreen_exit_output: FullscreenExitOutput,
    pub fullscreen_scrollbar: ScrollViewScrollbar,
    pub warnings: WarningSettings,
}

/// Callbacks for every setting.
pub struct SettingsCallbacks {
    pub on_auto_compact_change: Rc<dyn Fn(bool)>,
    pub on_show_images_change: Rc<dyn Fn(bool)>,
    pub on_image_width_cells_change: Rc<dyn Fn(i64)>,
    pub on_auto_resize_images_change: Rc<dyn Fn(bool)>,
    pub on_block_images_change: Rc<dyn Fn(bool)>,
    pub on_enable_skill_commands_change: Rc<dyn Fn(bool)>,
    pub on_steering_mode_change: Rc<dyn Fn(SteeringMode)>,
    pub on_follow_up_mode_change: Rc<dyn Fn(FollowUpMode)>,
    pub on_transport_change: Rc<dyn Fn(Transport)>,
    pub on_http_idle_timeout_ms_change: Rc<dyn Fn(u64)>,
    pub on_thinking_level_change: Rc<dyn Fn(ThinkingLevel)>,
    pub on_theme_change: Rc<dyn Fn(&JsString)>,
    pub on_theme_preview: Option<ThemePreviewFn>,
    pub on_hide_thinking_block_change: Rc<dyn Fn(bool)>,
    pub on_mermaid_rendering_mode_change: Rc<dyn Fn(MermaidRenderingMode)>,
    pub on_show_cache_miss_notices_change: Rc<dyn Fn(bool)>,
    pub on_collapse_changelog_change: Rc<dyn Fn(bool)>,
    pub on_enable_install_telemetry_change: Rc<dyn Fn(bool)>,
    pub on_double_escape_action_change: Rc<dyn Fn(crate::core::settings_manager::DoubleEscapeAction)>,
    pub on_tree_filter_mode_change: Rc<dyn Fn(TreeFilterMode)>,
    pub on_show_hardware_cursor_change: Rc<dyn Fn(bool)>,
    pub on_editor_padding_x_change: Rc<dyn Fn(i64)>,
    pub on_output_pad_change: Rc<dyn Fn(u8)>,
    pub on_autocomplete_max_visible_change: Rc<dyn Fn(i64)>,
    pub on_quiet_startup_change: Rc<dyn Fn(bool)>,
    pub on_default_project_trust_change: Rc<dyn Fn(DefaultProjectTrust)>,
    pub on_clear_on_shrink_change: Rc<dyn Fn(bool)>,
    pub on_show_terminal_progress_change: Rc<dyn Fn(bool)>,
    pub on_tui_mode_change: Rc<dyn Fn(TuiMode)>,
    pub on_fullscreen_exit_output_change: Rc<dyn Fn(FullscreenExitOutput)>,
    pub on_fullscreen_scrollbar_change: Rc<dyn Fn(ScrollViewScrollbar)>,
    pub on_warnings_change: Rc<dyn Fn(WarningSettings)>,
    pub on_cancel: Rc<dyn Fn()>,
}

fn bool_value(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn bool_values() -> Vec<JsString> {
    vec!["true".into(), "false".into()]
}

fn transport_name(transport: Transport) -> &'static str {
    match transport {
        Transport::Sse => "sse",
        Transport::Websocket => "websocket",
        Transport::WebsocketCached => "websocket-cached",
        Transport::Auto => "auto",
    }
}

fn parse_transport(value: &JsString) -> Option<Transport> {
    let Ok(value) = value.to_utf8() else {
        return None;
    };
    match value.as_str() {
        "sse" => Some(Transport::Sse),
        "websocket" => Some(Transport::Websocket),
        "websocket-cached" => Some(Transport::WebsocketCached),
        "auto" => Some(Transport::Auto),
        _ => None,
    }
}

fn mermaid_name(mode: MermaidRenderingMode) -> &'static str {
    match mode {
        MermaidRenderingMode::Off => "off",
        MermaidRenderingMode::Final => "final",
        MermaidRenderingMode::Streaming => "streaming",
    }
}

fn parse_mermaid(value: &JsString) -> Option<MermaidRenderingMode> {
    let Ok(value) = value.to_utf8() else {
        return None;
    };
    match value.as_str() {
        "off" => Some(MermaidRenderingMode::Off),
        "final" => Some(MermaidRenderingMode::Final),
        "streaming" => Some(MermaidRenderingMode::Streaming),
        _ => None,
    }
}

fn double_escape_name(action: crate::core::settings_manager::DoubleEscapeAction) -> &'static str {
    use crate::core::settings_manager::DoubleEscapeAction;
    match action {
        DoubleEscapeAction::Fork => "fork",
        DoubleEscapeAction::Tree => "tree",
        DoubleEscapeAction::None => "none",
    }
}

fn parse_double_escape(value: &JsString) -> Option<crate::core::settings_manager::DoubleEscapeAction> {
    use crate::core::settings_manager::DoubleEscapeAction;
    let Ok(value) = value.to_utf8() else {
        return None;
    };
    match value.as_str() {
        "fork" => Some(DoubleEscapeAction::Fork),
        "tree" => Some(DoubleEscapeAction::Tree),
        "none" => Some(DoubleEscapeAction::None),
        _ => None,
    }
}

fn tree_filter_name(mode: TreeFilterMode) -> &'static str {
    match mode {
        TreeFilterMode::Default => "default",
        TreeFilterMode::NoTools => "no-tools",
        TreeFilterMode::UserOnly => "user-only",
        TreeFilterMode::LabeledOnly => "labeled-only",
        TreeFilterMode::All => "all",
    }
}

fn parse_tree_filter(value: &JsString) -> Option<TreeFilterMode> {
    let Ok(value) = value.to_utf8() else {
        return None;
    };
    match value.as_str() {
        "default" => Some(TreeFilterMode::Default),
        "no-tools" => Some(TreeFilterMode::NoTools),
        "user-only" => Some(TreeFilterMode::UserOnly),
        "labeled-only" => Some(TreeFilterMode::LabeledOnly),
        "all" => Some(TreeFilterMode::All),
        _ => None,
    }
}

pub(crate) fn tui_mode_name(mode: TuiMode) -> &'static str {
    match mode {
        TuiMode::Regular => "regular",
        TuiMode::Fullscreen => "fullscreen",
    }
}

fn parse_tui_mode(value: &JsString) -> Option<TuiMode> {
    let Ok(value) = value.to_utf8() else {
        return None;
    };
    match value.as_str() {
        "regular" => Some(TuiMode::Regular),
        "fullscreen" => Some(TuiMode::Fullscreen),
        _ => None,
    }
}

fn fullscreen_exit_name(output: FullscreenExitOutput) -> &'static str {
    match output {
        FullscreenExitOutput::Transcript => "transcript",
        FullscreenExitOutput::ResumeHint => "resume-hint",
    }
}

fn parse_fullscreen_exit(value: &JsString) -> Option<FullscreenExitOutput> {
    let Ok(value) = value.to_utf8() else {
        return None;
    };
    match value.as_str() {
        "transcript" => Some(FullscreenExitOutput::Transcript),
        "resume-hint" => Some(FullscreenExitOutput::ResumeHint),
        _ => None,
    }
}

fn scrollbar_name(scrollbar: ScrollViewScrollbar) -> &'static str {
    match scrollbar {
        ScrollViewScrollbar::Auto => "auto",
        ScrollViewScrollbar::Always => "always",
        ScrollViewScrollbar::Hidden => "hidden",
    }
}

fn parse_scrollbar(value: &JsString) -> Option<ScrollViewScrollbar> {
    let Ok(value) = value.to_utf8() else {
        return None;
    };
    match value.as_str() {
        "auto" => Some(ScrollViewScrollbar::Auto),
        "always" => Some(ScrollViewScrollbar::Always),
        "hidden" => Some(ScrollViewScrollbar::Hidden),
        _ => None,
    }
}

fn theme_items(available_themes: &[JsString]) -> Vec<SelectItem> {
    available_themes
        .iter()
        .map(|name| SelectItem::new(name.clone(), name.clone()))
        .collect()
}

const AUTOMATIC_THEME_VALUE: &str = "/";

fn single_mode_theme_items(available_themes: &[JsString]) -> Vec<SelectItem> {
    let mut items = vec![
        SelectItem::new(AUTOMATIC_THEME_VALUE, "Automatic")
            .with_description("Use separate themes for light and dark terminal appearance"),
    ];
    items.extend(theme_items(available_themes));
    items
}

fn preferred_theme(
    available_themes: &[JsString],
    preferred: Option<&JsString>,
    fallback: impl Into<JsString>,
) -> JsString {
    let fallback = fallback.into();
    if let Some(preferred) = preferred
        && available_themes.iter().any(|name| name == preferred)
    {
        return preferred.into();
    }
    if available_themes.iter().any(|name| name == &fallback) {
        return fallback;
    }
    available_themes.first().cloned().unwrap_or(fallback)
}

fn default_automatic_themes(
    current_theme_setting: &JsString,
    available_themes: &[JsString],
) -> crate::modes::interactive::theme::AutoThemeSetting {
    if let Some(auto_theme) = parse_auto_theme_setting(Some(current_theme_setting)) {
        return auto_theme;
    }
    let current_fixed_theme = if current_theme_setting.contains('/') {
        None
    } else {
        Some(current_theme_setting)
    };
    let theme_name = preferred_theme(available_themes, current_fixed_theme, "dark");
    crate::modes::interactive::theme::AutoThemeSetting {
        light_theme: theme_name.clone(),
        dark_theme: theme_name,
    }
}

/// Simple select submenu used by the settings and theme dialogs.
pub struct SelectSubmenu {
    title: Text,
    description: Option<Text>,
    select_list: SelectList,
}

impl SelectSubmenu {
    pub fn new(
        title: impl Into<JsString>,
        description: impl Into<JsString>,
        options: Vec<SelectItem>,
        current_value: &JsString,
        on_select: Rc<dyn Fn(&JsString)>,
        on_cancel: Rc<dyn Fn()>,
        on_selection_change: Option<SelectionChangeFn>,
    ) -> Self {
        let title = title.into();
        let description = description.into();
        let theme = crate::modes::interactive::theme::current_theme();
        let current_index = options.iter().position(|item| &item.value == current_value);
        let max_visible = options.len().min(10);
        let mut select_list = SelectList::new(
            options,
            max_visible,
            get_select_list_theme(),
            SETTINGS_SUBMENU_SELECT_LIST_LAYOUT,
        );
        if let Some(index) = current_index {
            select_list.set_selected_index(index as isize);
        }
        select_list.on_select = Some(Rc::new(move |item: &SelectItem| on_select(&item.value)));
        select_list.on_cancel = Some(on_cancel);
        if let Some(on_selection_change) = on_selection_change {
            select_list.on_selection_change = Some(Rc::new(move |item: &SelectItem| {
                on_selection_change(&item.value);
            }));
        }
        Self {
            title: Text::new(theme.bold(&theme.fg(ThemeColor::Accent, title)), 0, 0),
            description: if description.is_empty() {
                None
            } else {
                Some(Text::new(theme.fg(ThemeColor::Muted, description), 0, 0))
            },
            select_list,
        }
    }

    pub fn select_list_mut(&mut self) -> &mut SelectList {
        &mut self.select_list
    }
}

impl Component for SelectSubmenu {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = self.title.render(width);
        if let Some(description) = &mut self.description {
            lines.extend(Spacer::new(1).render(width));
            lines.extend(description.render(width));
        }
        lines.extend(Spacer::new(1).render(width));
        lines.extend(self.select_list.render(width));
        lines.extend(Spacer::new(1).render(width));
        lines.extend(
            Text::new(
                (get_settings_list_theme().description)(&"  Enter to select · Esc to go back".into()),
                0,
                0,
            )
            .render(width),
        );
        lines
    }

    fn handle_input(&mut self, data: &JsString) {
        self.select_list.handle_input(data);
    }
}

/// Warning-settings submenu.
pub struct WarningSettingsSubmenu {
    settings_list: SettingsList,
}

impl WarningSettingsSubmenu {
    pub fn new(warnings: &WarningSettings, on_change: Rc<dyn Fn(WarningSettings)>, on_cancel: Rc<dyn Fn()>) -> Self {
        let state = RefCell::new(warnings.clone());
        let items = vec![SettingItem {
            id: "anthropic-extra-usage".into(),
            label: "Anthropic extra usage".into(),
            description: Some("Warn when Anthropic subscription auth may use paid extra usage".into()),
            current_value: bool_value(state.borrow().anthropic_extra_usage.unwrap_or(true)).into(),
            values: Some(bool_values()),
            submenu: None,
        }];
        let settings_list = SettingsList::new(
            items,
            items_len(1),
            get_settings_list_theme(),
            Rc::new(move |id: &JsString, new_value: &JsString| {
                if id == "anthropic-extra-usage" {
                    let mut state = state.borrow_mut();
                    state.anthropic_extra_usage = Some(new_value == "true");
                    on_change(state.clone());
                }
            }),
            on_cancel,
            SettingsListOptions::default(),
        );
        Self { settings_list }
    }
}

fn items_len(len: usize) -> usize {
    len.min(10)
}

impl Component for WarningSettingsSubmenu {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.settings_list.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.settings_list.handle_input(data);
    }
}

/// Theme submenu with single/automatic modes.
pub struct ThemeSubmenu {
    screen: Rc<std::cell::Cell<ThemeMode>>,
    state: Rc<RefCell<ThemeState>>,
    single_list: SelectList,
    automatic_list: SettingsList,
    needs_single_rebuild: Rc<std::cell::Cell<bool>>,
    needs_automatic_rebuild: Rc<std::cell::Cell<bool>>,
    callbacks: Rc<SettingsCallbacks>,
    available_themes: Vec<JsString>,
    terminal_theme: TerminalTheme,
    on_done: ThemeDoneFn,
    original_theme_setting: JsString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeMode {
    Single,
    Automatic,
}

struct ThemeState {
    mode: ThemeMode,
    single_theme: JsString,
    light_theme: JsString,
    dark_theme: JsString,
}

fn theme_setting(state: &ThemeState) -> JsString {
    match state.mode {
        ThemeMode::Automatic => automatic_setting(state),
        ThemeMode::Single => state.single_theme.clone(),
    }
}

fn automatic_setting(state: &ThemeState) -> JsString {
    js_format!("{}/{}", state.light_theme, state.dark_theme)
}

fn active_automatic_theme(state: &ThemeState, terminal_theme: TerminalTheme) -> JsString {
    if terminal_theme == TerminalTheme::Light {
        state.light_theme.clone()
    } else {
        state.dark_theme.clone()
    }
}

impl ThemeSubmenu {
    pub fn new(
        current_theme_setting: &JsString,
        terminal_theme: TerminalTheme,
        available_themes: Vec<JsString>,
        callbacks: Rc<SettingsCallbacks>,
        on_done: ThemeDoneFn,
    ) -> Self {
        let auto_theme = parse_auto_theme_setting(Some(current_theme_setting));
        let automatic_themes = default_automatic_themes(current_theme_setting, &available_themes);
        let fixed_theme = if auto_theme.is_some() || current_theme_setting.contains('/') {
            None
        } else {
            Some(current_theme_setting)
        };
        let mode = if auto_theme.is_some() {
            ThemeMode::Automatic
        } else {
            ThemeMode::Single
        };
        let light_theme = automatic_themes.light_theme.clone();
        let dark_theme = automatic_themes.dark_theme.clone();
        let active_automatic = if terminal_theme == TerminalTheme::Light {
            &light_theme
        } else {
            &dark_theme
        };
        let single_theme = preferred_theme(
            &available_themes,
            fixed_theme.or(if auto_theme.is_some() {
                Some(&active_automatic)
            } else {
                None
            }),
            "dark",
        );
        let state = Rc::new(RefCell::new(ThemeState {
            mode,
            single_theme,
            light_theme,
            dark_theme,
        }));
        let screen = Rc::new(std::cell::Cell::new(mode));
        let needs_single_rebuild = Rc::new(std::cell::Cell::new(false));
        let needs_automatic_rebuild = Rc::new(std::cell::Cell::new(false));
        let mut submenu = Self {
            screen,
            state,
            single_list: SelectList::new(
                Vec::new(),
                1,
                get_select_list_theme(),
                SETTINGS_SUBMENU_SELECT_LIST_LAYOUT,
            ),
            automatic_list: SettingsList::new(
                Vec::new(),
                1,
                get_settings_list_theme(),
                Rc::new(|_, _| {}),
                Rc::new(|| {}),
                SettingsListOptions::default(),
            ),
            needs_single_rebuild,
            needs_automatic_rebuild,
            callbacks,
            available_themes,
            terminal_theme,
            on_done,
            original_theme_setting: current_theme_setting.into(),
        };
        submenu.rebuild_single_list();
        submenu.rebuild_automatic_list();
        submenu
    }

    fn rebuild_single_list(&mut self) {
        let state = self.state.clone();
        let screen = self.screen.clone();
        let needs_automatic_rebuild = self.needs_automatic_rebuild.clone();
        let callbacks = self.callbacks.clone();
        let on_done = self.on_done.clone();
        let original = self.original_theme_setting.clone();
        let current = state.borrow().single_theme.clone();
        let select_state = state.clone();
        let select_callbacks = callbacks.clone();
        let select_on_done = on_done.clone();
        let change_state = state.clone();
        let change_callbacks = callbacks.clone();
        let options = single_mode_theme_items(&self.available_themes);
        let current_index = options.iter().position(|item| item.value == current);
        let max_visible = options.len().clamp(1, 10);
        let mut list = SelectList::new(
            options,
            max_visible,
            get_select_list_theme(),
            SETTINGS_SUBMENU_SELECT_LIST_LAYOUT,
        );
        if let Some(index) = current_index {
            list.set_selected_index(index as isize);
        }
        list.on_select = Some(Rc::new(move |item: &SelectItem| {
            if item.value == AUTOMATIC_THEME_VALUE {
                let mut state = select_state.borrow_mut();
                state.mode = ThemeMode::Automatic;
                let setting = automatic_setting(&state);
                drop(state);
                screen.set(ThemeMode::Automatic);
                needs_automatic_rebuild.set(true);
                if let Some(preview) = &select_callbacks.on_theme_preview {
                    preview(&setting);
                }
                return;
            }
            select_state.borrow_mut().single_theme = item.value.clone();
            (select_on_done)(Some(&item.value));
        }));
        list.on_cancel = Some({
            let callbacks = callbacks.clone();
            let on_done = on_done.clone();
            Rc::new(move || {
                if let Some(preview) = &callbacks.on_theme_preview {
                    preview(&original);
                }
                (on_done)(None);
            })
        });
        list.on_selection_change = Some(Rc::new(move |item: &SelectItem| {
            let setting = if item.value == AUTOMATIC_THEME_VALUE {
                automatic_setting(&change_state.borrow())
            } else {
                item.value.clone()
            };
            if let Some(preview) = &change_callbacks.on_theme_preview {
                preview(&setting);
            }
        }));
        self.single_list = list;
        self.needs_single_rebuild.set(false);
    }

    fn rebuild_automatic_list(&mut self) {
        let state = self.state.clone();
        let screen = self.screen.clone();
        let needs_single_rebuild = self.needs_single_rebuild.clone();
        let callbacks = self.callbacks.clone();
        let on_done = self.on_done.clone();
        let original = self.original_theme_setting.clone();
        let available_themes = self.available_themes.clone();
        let terminal_theme = self.terminal_theme;
        let current = state.borrow();
        let items = vec![
            SettingItem {
                id: "light-theme".into(),
                label: "Light theme".into(),
                description: Some("Theme to use in automatic mode when the terminal is light".into()),
                current_value: current.light_theme.clone(),
                values: None,
                submenu: Some(Rc::new({
                    let state = state.clone();
                    let callbacks = callbacks.clone();
                    let available_themes = available_themes.clone();
                    move |current_value: &JsString, done: SubmenuDone| {
                        let state = state.clone();
                        let callbacks = callbacks.clone();
                        let preview_callbacks = callbacks.clone();
                        let select_state = state.clone();
                        let select_callbacks = callbacks.clone();
                        let select_done = done.clone();
                        let done_cancel = done.clone();
                        let cancel_callbacks = callbacks.clone();
                        component(SelectSubmenu::new(
                            "Light Theme",
                            "Select the theme to use for light terminal appearance",
                            theme_items(&available_themes),
                            current_value,
                            Rc::new(move |value: &JsString| {
                                select_state.borrow_mut().light_theme = value.into();
                                if let Some(preview) = &select_callbacks.on_theme_preview {
                                    preview(&theme_setting(&select_state.borrow()));
                                }
                                (select_done)(Some(value.into()));
                            }),
                            Rc::new(move || {
                                if let Some(preview) = &cancel_callbacks.on_theme_preview {
                                    preview(&theme_setting(&state.borrow()));
                                }
                                (done_cancel)(None);
                            }),
                            Some(Rc::new(move |value: &JsString| {
                                if let Some(preview) = &preview_callbacks.on_theme_preview {
                                    preview(value);
                                }
                            })),
                        ))
                    }
                })),
            },
            SettingItem {
                id: "dark-theme".into(),
                label: "Dark theme".into(),
                description: Some("Theme to use in automatic mode when the terminal is dark".into()),
                current_value: current.dark_theme.clone(),
                values: None,
                submenu: Some(Rc::new({
                    let state = state.clone();
                    let callbacks = callbacks.clone();
                    let available_themes = available_themes.clone();
                    move |current_value: &JsString, done: SubmenuDone| {
                        let state = state.clone();
                        let callbacks = callbacks.clone();
                        let preview_callbacks = callbacks.clone();
                        let select_state = state.clone();
                        let select_callbacks = callbacks.clone();
                        let select_done = done.clone();
                        let done_cancel = done.clone();
                        let cancel_callbacks = callbacks.clone();
                        component(SelectSubmenu::new(
                            "Dark Theme",
                            "Select the theme to use for dark terminal appearance",
                            theme_items(&available_themes),
                            current_value,
                            Rc::new(move |value: &JsString| {
                                select_state.borrow_mut().dark_theme = value.into();
                                if let Some(preview) = &select_callbacks.on_theme_preview {
                                    preview(&theme_setting(&select_state.borrow()));
                                }
                                (select_done)(Some(value.into()));
                            }),
                            Rc::new(move || {
                                if let Some(preview) = &cancel_callbacks.on_theme_preview {
                                    preview(&theme_setting(&state.borrow()));
                                }
                                (done_cancel)(None);
                            }),
                            Some(Rc::new(move |value: &JsString| {
                                if let Some(preview) = &preview_callbacks.on_theme_preview {
                                    preview(value);
                                }
                            })),
                        ))
                    }
                })),
            },
            SettingItem {
                id: "apply".into(),
                label: "Apply".into(),
                description: Some("Save and go back".into()),
                current_value: "save and go back".into(),
                values: Some(vec!["save and go back".into()]),
                submenu: None,
            },
            SettingItem {
                id: "single-mode".into(),
                label: "Change mode".into(),
                description: Some("Switch to one theme for light and dark".into()),
                current_value: "switch to single theme".into(),
                values: Some(vec!["switch to single theme".into()]),
                submenu: None,
            },
        ];
        drop(current);
        let change_state = state.clone();
        let change_callbacks = callbacks.clone();
        let change_on_done = on_done.clone();
        let cancel_callbacks = callbacks.clone();
        let cancel_on_done = on_done.clone();
        let mut list = SettingsList::new(
            items,
            10,
            get_settings_list_theme(),
            Rc::new(move |id: &JsString, _new_value: &JsString| {
                let Ok(id) = id.to_utf8() else {
                    return;
                };
                match id.as_str() {
                    "single-mode" => {
                        let mut state = change_state.borrow_mut();
                        state.mode = ThemeMode::Single;
                        state.single_theme = active_automatic_theme(&state, terminal_theme);
                        let setting = state.single_theme.clone();
                        drop(state);
                        screen.set(ThemeMode::Single);
                        needs_single_rebuild.set(true);
                        if let Some(preview) = &change_callbacks.on_theme_preview {
                            preview(&setting);
                        }
                    }
                    "apply" => {
                        let setting = automatic_setting(&change_state.borrow());
                        (change_on_done)(Some(&setting));
                    }
                    _ => {}
                }
            }),
            Rc::new(move || {
                if let Some(preview) = &cancel_callbacks.on_theme_preview {
                    preview(&original);
                }
                (cancel_on_done)(None);
            }),
            SettingsListOptions::default(),
        );
        let _ = &mut list;
        self.automatic_list = list;
        self.needs_automatic_rebuild.set(false);
    }
}

impl Component for ThemeSubmenu {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        if self.needs_single_rebuild.get() {
            self.rebuild_single_list();
        }
        if self.needs_automatic_rebuild.get() {
            self.rebuild_automatic_list();
        }
        let theme = crate::modes::interactive::theme::current_theme();
        match self.screen.get() {
            ThemeMode::Single => {
                let mut lines = Text::new(theme.bold(&theme.fg(ThemeColor::Accent, "Theme")), 0, 0).render(width);
                lines.extend(Spacer::new(1).render(width));
                lines.extend(
                    Text::new(
                        theme.fg(
                            ThemeColor::Muted,
                            "Select a theme, or choose Automatic to follow terminal appearance.",
                        ),
                        0,
                        0,
                    )
                    .render(width),
                );
                lines.extend(Spacer::new(1).render(width));
                lines.extend(self.single_list.render(width));
                lines.extend(Spacer::new(1).render(width));
                lines.extend(
                    Text::new(
                        (get_settings_list_theme().description)(&"  Enter to select · Esc to go back".into()),
                        0,
                        0,
                    )
                    .render(width),
                );
                lines
            }
            ThemeMode::Automatic => {
                let mut lines =
                    Text::new(theme.bold(&theme.fg(ThemeColor::Accent, "Automatic Theme")), 0, 0).render(width);
                lines.extend(Spacer::new(1).render(width));
                lines.extend(
                    Text::new(
                        theme.fg(
                            ThemeColor::Muted,
                            "Choose themes for terminal light and dark appearance.",
                        ),
                        0,
                        0,
                    )
                    .render(width),
                );
                lines.extend(
                    Text::new(
                        theme.fg(ThemeColor::Muted, "Light/dark detection requires terminal support."),
                        0,
                        0,
                    )
                    .render(width),
                );
                lines.extend(Spacer::new(1).render(width));
                lines.extend(self.automatic_list.render(width));
                lines
            }
        }
    }

    fn handle_input(&mut self, data: &JsString) {
        if self.screen.get() != self.state.borrow().mode {
            self.screen.set(self.state.borrow().mode);
        }
        match self.screen.get() {
            ThemeMode::Single => self.single_list.handle_input(data),
            ThemeMode::Automatic => self.automatic_list.handle_input(data),
        }
    }
}

/// Main settings selector.
pub struct SettingsSelectorComponent {
    settings_list: SettingsList,
}

impl SettingsSelectorComponent {
    pub fn new(config: SettingsConfig, callbacks: Rc<SettingsCallbacks>) -> Self {
        let supports_images = pi_tui::terminal_image::get_capabilities().images.is_some();
        let follow_up_key = key_display_text("app.message.followUp");
        let current_warnings = Rc::new(RefCell::new(config.warnings.clone()));

        let mut items = base_items(&config, &callbacks, &follow_up_key, current_warnings.clone());

        if supports_images {
            items.insert(
                1,
                SettingItem {
                    id: "show-images".into(),
                    label: "Show images".into(),
                    description: Some("Render images inline in terminal".into()),
                    current_value: bool_value(config.show_images).into(),
                    values: Some(bool_values()),
                    submenu: None,
                },
            );
            items.insert(
                2,
                SettingItem {
                    id: "image-width-cells".into(),
                    label: "Image width".into(),
                    description: Some("Preferred inline image width in terminal cells".into()),
                    current_value: js_format!("{}", config.image_width_cells),
                    values: Some(vec!["60".into(), "80".into(), "120".into()]),
                    submenu: None,
                },
            );
        }

        let auto_resize_index = if supports_images { 3 } else { 1 };
        items.insert(
            auto_resize_index,
            SettingItem {
                id: "auto-resize-images".into(),
                label: "Auto-resize images".into(),
                description: Some("Resize large images to 2000x2000 max for better model compatibility".into()),
                current_value: bool_value(config.auto_resize_images).into(),
                values: Some(bool_values()),
                submenu: None,
            },
        );
        insert_after(
            &mut items,
            "auto-resize-images",
            SettingItem {
                id: "block-images".into(),
                label: "Block images".into(),
                description: Some("Prevent images from being sent to LLM providers".into()),
                current_value: bool_value(config.block_images).into(),
                values: Some(bool_values()),
                submenu: None,
            },
        );
        insert_after(
            &mut items,
            "block-images",
            SettingItem {
                id: "skill-commands".into(),
                label: "Skill commands".into(),
                description: Some("Register skills as /skill:name commands".into()),
                current_value: bool_value(config.enable_skill_commands).into(),
                values: Some(bool_values()),
                submenu: None,
            },
        );
        insert_after(
            &mut items,
            "skill-commands",
            SettingItem {
                id: "show-hardware-cursor".into(),
                label: "Show hardware cursor".into(),
                description: Some("Show the terminal cursor while still positioning it for IME support".into()),
                current_value: bool_value(config.show_hardware_cursor).into(),
                values: Some(bool_values()),
                submenu: None,
            },
        );
        insert_after(
            &mut items,
            "show-hardware-cursor",
            SettingItem {
                id: "editor-padding".into(),
                label: "Editor padding".into(),
                description: Some("Horizontal padding for input editor (0-3)".into()),
                current_value: js_format!("{}", config.editor_padding_x),
                values: Some(vec!["0".into(), "1".into(), "2".into(), "3".into()]),
                submenu: None,
            },
        );
        insert_after(
            &mut items,
            "editor-padding",
            SettingItem {
                id: "output-padding".into(),
                label: "Output padding".into(),
                description: Some("Horizontal padding for user messages, assistant messages, and thinking".into()),
                current_value: js_format!("{}", config.output_pad),
                values: Some(vec!["0".into(), "1".into()]),
                submenu: None,
            },
        );
        insert_after(
            &mut items,
            "output-padding",
            SettingItem {
                id: "autocomplete-max-visible".into(),
                label: "Autocomplete max items".into(),
                description: Some("Max visible items in autocomplete dropdown (3-20)".into()),
                current_value: js_format!("{}", config.autocomplete_max_visible),
                values: Some(vec![
                    "3".into(),
                    "5".into(),
                    "7".into(),
                    "10".into(),
                    "15".into(),
                    "20".into(),
                ]),
                submenu: None,
            },
        );
        insert_after(
            &mut items,
            "autocomplete-max-visible",
            SettingItem {
                id: "clear-on-shrink".into(),
                label: "Clear on shrink".into(),
                description: Some("Clear empty rows when content shrinks (may cause flicker)".into()),
                current_value: bool_value(config.clear_on_shrink).into(),
                values: Some(bool_values()),
                submenu: None,
            },
        );
        insert_after(
            &mut items,
            "clear-on-shrink",
            SettingItem {
                id: "terminal-progress".into(),
                label: "Terminal progress".into(),
                description: Some("Show OSC 9;4 progress indicators in the terminal tab bar".into()),
                current_value: bool_value(config.show_terminal_progress).into(),
                values: Some(bool_values()),
                submenu: None,
            },
        );

        let settings_list = SettingsList::new(
            items,
            10,
            get_settings_list_theme(),
            build_change_handler(callbacks.clone()),
            callbacks.on_cancel.clone(),
            SettingsListOptions { enable_search: true },
        );
        Self { settings_list }
    }

    pub fn get_settings_list(&self) -> &SettingsList {
        &self.settings_list
    }

    pub fn get_settings_list_mut(&mut self) -> &mut SettingsList {
        &mut self.settings_list
    }
}

impl Component for SettingsSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = dynamic_border().render(width);
        lines.extend(self.settings_list.render(width));
        lines.extend(dynamic_border().render(width));
        lines
    }

    fn handle_input(&mut self, data: &JsString) {
        self.settings_list.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.settings_list.invalidate();
    }
}

fn insert_after(items: &mut Vec<SettingItem>, after_id: &str, item: SettingItem) {
    if let Some(index) = items.iter().position(|existing| existing.id == after_id) {
        items.insert(index + 1, item);
    }
}

fn base_items(
    config: &SettingsConfig,
    callbacks: &Rc<SettingsCallbacks>,
    follow_up_key: &JsString,
    current_warnings: Rc<RefCell<WarningSettings>>,
) -> Vec<SettingItem> {
    let callbacks_for_warnings = callbacks.clone();
    let callbacks_for_thinking = callbacks.clone();
    let callbacks_for_theme = callbacks.clone();
    let config_available_themes = config.available_themes.clone();
    let terminal_theme = config.terminal_theme;
    let current_theme = config.current_theme.clone();
    let thinking_levels = config.available_thinking_levels.clone();
    vec![
        SettingItem {
            id: "autocompact".into(),
            label: "Auto-compact".into(),
            description: Some("Automatically compact context when it gets too large".into()),
            current_value: bool_value(config.auto_compact).into(),
            values: Some(bool_values()),
            submenu: None,
        },
        SettingItem {
            id: "steering-mode".into(),
            label: "Steering mode".into(),
            description: Some(
                "Enter while streaming queues steering messages. 'one-at-a-time': deliver one, wait for response. 'all': deliver all at once."
                    .into(),
            ),
            current_value: config.steering_mode.as_str().into(),
            values: Some(vec!["one-at-a-time".into(), "all".into()]),
            submenu: None,
        },
        SettingItem {
            id: "follow-up-mode".into(),
            label: "Follow-up mode".into(),
            description: Some(js_format!(
                "{} queues follow-up messages until agent stops. 'one-at-a-time': deliver one, wait for response. 'all': deliver all at once.", follow_up_key
            )),
            current_value: config.follow_up_mode.as_str().into(),
            values: Some(vec!["one-at-a-time".into(), "all".into()]),
            submenu: None,
        },
        SettingItem {
            id: "transport".into(),
            label: "Transport".into(),
            description: Some("Preferred transport for providers that support multiple transports".into()),
            current_value: transport_name(config.transport).into(),
            values: Some(vec![
                "sse".into(),
                "websocket".into(),
                "websocket-cached".into(),
                "auto".into(),
            ]),
            submenu: None,
        },
        SettingItem {
            id: "http-idle-timeout".into(),
            label: "HTTP idle timeout".into(),
            description: Some(
                "Maximum idle gap while waiting for HTTP headers or body chunks. Disable for local models that pause longer than five minutes."
                    .into(),
            ),
            current_value: format_http_idle_timeout_ms(config.http_idle_timeout_ms).into(),
            values: Some(
                HTTP_IDLE_TIMEOUT_CHOICES
                    .iter()
                    .map(|(label, _)| (*label).into())
                    .collect(),
            ),
            submenu: None,
        },
        SettingItem {
            id: "hide-thinking".into(),
            label: "Hide thinking".into(),
            description: Some("Hide thinking blocks in assistant responses".into()),
            current_value: bool_value(config.hide_thinking_block).into(),
            values: Some(bool_values()),
            submenu: None,
        },
        SettingItem {
            id: "mermaid-rendering".into(),
            label: "Mermaid diagrams".into(),
            description: Some("Render Mermaid code blocks as Unicode diagrams".into()),
            current_value: mermaid_name(config.mermaid_rendering_mode).into(),
            values: Some(vec!["off".into(), "final".into(), "streaming".into()]),
            submenu: None,
        },
        SettingItem {
            id: "cache-miss-notices".into(),
            label: "Cache miss notices".into(),
            description: Some("Show transcript notices for significant prompt-cache misses".into()),
            current_value: bool_value(config.show_cache_miss_notices).into(),
            values: Some(bool_values()),
            submenu: None,
        },
        SettingItem {
            id: "collapse-changelog".into(),
            label: "Collapse changelog".into(),
            description: Some("Show condensed changelog after updates".into()),
            current_value: bool_value(config.collapse_changelog).into(),
            values: Some(bool_values()),
            submenu: None,
        },
        SettingItem {
            id: "quiet-startup".into(),
            label: "Quiet startup".into(),
            description: Some("Disable verbose printing at startup".into()),
            current_value: bool_value(config.quiet_startup).into(),
            values: Some(bool_values()),
            submenu: None,
        },
        SettingItem {
            id: "install-telemetry".into(),
            label: "Install telemetry".into(),
            description: Some("Send an anonymous version/update ping after changelog-detected updates".into()),
            current_value: bool_value(config.enable_install_telemetry).into(),
            values: Some(bool_values()),
            submenu: None,
        },
        SettingItem {
            id: "default-project-trust".into(),
            label: "Default project trust".into(),
            description: Some(
                "Fallback behavior when no extension or saved trust decision decides project trust".into(),
            ),
            current_value: default_project_trust_label(config.default_project_trust).into(),
            values: Some(default_project_trust_labels()),
            submenu: None,
        },
        SettingItem {
            id: "double-escape-action".into(),
            label: "Double-escape action".into(),
            description: Some("Action when pressing Escape twice with empty editor".into()),
            current_value: double_escape_name(config.double_escape_action).into(),
            values: Some(vec!["tree".into(), "fork".into(), "none".into()]),
            submenu: None,
        },
        SettingItem {
            id: "tree-filter-mode".into(),
            label: "Tree filter mode".into(),
            description: Some("Default filter when opening /tree".into()),
            current_value: tree_filter_name(config.tree_filter_mode).into(),
            values: Some(vec![
                "default".into(),
                "no-tools".into(),
                "user-only".into(),
                "labeled-only".into(),
                "all".into(),
            ]),
            submenu: None,
        },
        SettingItem {
            id: "warnings".into(),
            label: "Warnings".into(),
            description: Some("Enable or disable individual warnings".into()),
            current_value: "configure".into(),
            values: None,
            submenu: Some(Rc::new(move |_current: &JsString, done: SubmenuDone| {
                let warnings = current_warnings.borrow().clone();
                let on_change = callbacks_for_warnings.on_warnings_change.clone();
                component(WarningSettingsSubmenu::new(
                    &warnings,
                    Rc::new(move |warnings| on_change(warnings)),
                    {
                        let done = done.clone();
                        Rc::new(move || done(None))
                    },
                ))
            })),
        },
        SettingItem {
            id: "thinking".into(),
            label: "Thinking level".into(),
            description: Some("Reasoning depth for thinking-capable models".into()),
            current_value: config.thinking_level.as_str().into(),
            values: None,
            submenu: Some(Rc::new(move |current_value: &JsString, done: SubmenuDone| {
                let options: Vec<SelectItem> = thinking_levels
                    .iter()
                    .map(|level| {
                        SelectItem::new(level.as_str(), level.as_str()).with_description(thinking_description(*level))
                    })
                    .collect();
                let on_change = callbacks_for_thinking.on_thinking_level_change.clone();
                let done_for_select = done.clone();
                let thinking_levels = thinking_levels.clone();
                component(SelectSubmenu::new(
                    "Thinking Level",
                    "Select reasoning depth for thinking-capable models",
                    options,
                    current_value,
                    Rc::new(move |value: &JsString| {
                        if let Some(level) = thinking_levels.iter().find(|level| value == level.as_str()) {
                            on_change(*level);
                            done_for_select(Some(value.into()));
                        }
                    }),
                    {
                        let done = done.clone();
                        Rc::new(move || done(None))
                    },
                    None,
                ))
            })),
        },
        SettingItem {
            id: "tui-mode".into(),
            label: "TUI mode".into(),
            description: Some("Interface layout; fullscreen mode is experimental".into()),
            current_value: tui_mode_name(config.tui_mode).into(),
            values: Some(vec!["regular".into(), "fullscreen".into()]),
            submenu: None,
        },
        SettingItem {
            id: "fullscreen-exit-output".into(),
            label: "Fullscreen exit output".into(),
            description: Some(
                "Print the transcript or only a session resume hint when exiting fullscreen mode".into(),
            ),
            current_value: fullscreen_exit_name(config.fullscreen_exit_output).into(),
            values: Some(vec!["transcript".into(), "resume-hint".into()]),
            submenu: None,
        },
        SettingItem {
            id: "fullscreen-scrollbar".into(),
            label: "Fullscreen scrollbar".into(),
            description: Some("Scrollbar behavior in fullscreen mode; has no effect in regular mode".into()),
            current_value: scrollbar_name(config.fullscreen_scrollbar).into(),
            values: Some(vec!["auto".into(), "always".into(), "hidden".into()]),
            submenu: None,
        },
        SettingItem {
            id: "theme".into(),
            label: "Theme".into(),
            description: Some("Color theme for the interface".into()),
            current_value: current_theme.clone(),
            values: None,
            submenu: Some(Rc::new(move |current_value: &JsString, done: SubmenuDone| {
                let callbacks = callbacks_for_theme.clone();
                let on_done = done.clone();
                let submenu = ThemeSubmenu::new(
                    current_value,
                    terminal_theme,
                    config_available_themes.clone(),
                    callbacks,
                    Rc::new(move |value: Option<&JsString>| on_done(value.cloned())),
                );
                component(submenu)
            })),
        },
    ]
}

fn build_change_handler(callbacks: Rc<SettingsCallbacks>) -> SettingsChangeHandler {
    Rc::new(move |id: &JsString, new_value: &JsString| {
        let boolean = new_value == "true";
        let Ok(id) = id.to_utf8() else {
            return;
        };
        match id.as_str() {
            "autocompact" => (callbacks.on_auto_compact_change)(boolean),
            "show-images" => (callbacks.on_show_images_change)(boolean),
            "image-width-cells" => (callbacks.on_image_width_cells_change)(parse_int(new_value.units(), 10) as i64),
            "auto-resize-images" => (callbacks.on_auto_resize_images_change)(boolean),
            "block-images" => (callbacks.on_block_images_change)(boolean),
            "skill-commands" => (callbacks.on_enable_skill_commands_change)(boolean),
            "steering-mode" => {
                if let Some(mode) = SteeringMode::parse(new_value) {
                    (callbacks.on_steering_mode_change)(mode);
                }
            }
            "follow-up-mode" => {
                if let Some(mode) = SteeringMode::parse(new_value) {
                    (callbacks.on_follow_up_mode_change)(mode);
                }
            }
            "transport" => {
                if let Some(transport) = parse_transport(new_value) {
                    (callbacks.on_transport_change)(transport);
                }
            }
            "http-idle-timeout" => {
                if let Some((_, timeout_ms)) = HTTP_IDLE_TIMEOUT_CHOICES.iter().find(|(label, _)| new_value == *label) {
                    (callbacks.on_http_idle_timeout_ms_change)(*timeout_ms);
                }
            }
            "hide-thinking" => (callbacks.on_hide_thinking_block_change)(boolean),
            "mermaid-rendering" => {
                if let Some(mode) = parse_mermaid(new_value) {
                    (callbacks.on_mermaid_rendering_mode_change)(mode);
                }
            }
            "cache-miss-notices" => (callbacks.on_show_cache_miss_notices_change)(boolean),
            "collapse-changelog" => (callbacks.on_collapse_changelog_change)(boolean),
            "quiet-startup" => (callbacks.on_quiet_startup_change)(boolean),
            "install-telemetry" => (callbacks.on_enable_install_telemetry_change)(boolean),
            "default-project-trust" => {
                if let Some(trust) = default_project_trust_from_label(new_value) {
                    (callbacks.on_default_project_trust_change)(trust);
                }
            }
            "double-escape-action" => {
                if let Some(action) = parse_double_escape(new_value) {
                    (callbacks.on_double_escape_action_change)(action);
                }
            }
            "tree-filter-mode" => {
                if let Some(mode) = parse_tree_filter(new_value) {
                    (callbacks.on_tree_filter_mode_change)(mode);
                }
            }
            "show-hardware-cursor" => (callbacks.on_show_hardware_cursor_change)(boolean),
            "editor-padding" => (callbacks.on_editor_padding_x_change)(parse_int(new_value.units(), 10) as i64),
            "output-padding" => (callbacks.on_output_pad_change)(if new_value == "0" { 0 } else { 1 }),
            "autocomplete-max-visible" => {
                (callbacks.on_autocomplete_max_visible_change)(parse_int(new_value.units(), 10) as i64);
            }
            "clear-on-shrink" => (callbacks.on_clear_on_shrink_change)(boolean),
            "terminal-progress" => (callbacks.on_show_terminal_progress_change)(boolean),
            "tui-mode" => {
                if let Some(mode) = parse_tui_mode(new_value) {
                    (callbacks.on_tui_mode_change)(mode);
                }
            }
            "fullscreen-exit-output" => {
                if let Some(output) = parse_fullscreen_exit(new_value) {
                    (callbacks.on_fullscreen_exit_output_change)(output);
                }
            }
            "fullscreen-scrollbar" => {
                if let Some(scrollbar) = parse_scrollbar(new_value) {
                    (callbacks.on_fullscreen_scrollbar_change)(scrollbar);
                }
            }
            "theme" => (callbacks.on_theme_change)(new_value),
            _ => {}
        }
    })
}
