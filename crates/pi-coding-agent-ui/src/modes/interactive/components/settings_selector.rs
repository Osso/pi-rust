//! Ported from `coding-agent/src/modes/interactive/components/settings-selector.ts`.

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::types::Transport;
use pi_js_compat::{
    js_format,
    js_json::{JsString, JsValue},
    number::{js_number_to_string, parse_int},
};
use pi_tui::components::settings_list::{SettingItem, SettingsList, SettingsListOptions, SubmenuDone};
use pi_tui::tui::{ComponentRef, Container, component};
use pi_tui::{Component, SelectItem, SelectList, SelectListLayoutOptions, Spacer, Text};

use crate::core::http_dispatcher::{HTTP_IDLE_TIMEOUT_CHOICES, format_http_idle_timeout_ms};
use crate::core::settings_manager::{DefaultProjectTrust, Settings, TreeFilterMode};
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
    pub image_width_cells: f64,
    pub auto_resize_images: bool,
    pub block_images: bool,
    pub enable_skill_commands: bool,
    pub steering_mode: SteeringMode,
    pub follow_up_mode: FollowUpMode,
    pub transport: Transport,
    pub http_idle_timeout_ms: f64,
    pub thinking_level: ThinkingLevel,
    pub available_thinking_levels: Vec<ThinkingLevel>,
    pub current_theme: JsString,
    pub terminal_theme: TerminalTheme,
    pub available_themes: Vec<JsString>,
    pub hide_thinking_block: bool,
    pub hide_tool_output: bool,
    pub hide_coordination_messages: bool,
    pub collapse_changelog: bool,
    pub enable_install_telemetry: bool,
    pub double_escape_action: crate::core::settings_manager::DoubleEscapeAction,
    pub tree_filter_mode: TreeFilterMode,
    pub show_hardware_cursor: bool,
    pub editor_padding_x: f64,
    pub output_pad: u8,
    pub autocomplete_max_visible: f64,
    pub quiet_startup: bool,
    pub default_project_trust: DefaultProjectTrust,
    pub clear_on_shrink: bool,
    pub show_terminal_progress: bool,
    pub warnings: Settings,
}

/// Callbacks for every setting.
pub struct SettingsCallbacks {
    pub on_auto_compact_change: Rc<dyn Fn(bool)>,
    pub on_show_images_change: Rc<dyn Fn(bool)>,
    pub on_image_width_cells_change: Rc<dyn Fn(f64)>,
    pub on_auto_resize_images_change: Rc<dyn Fn(bool)>,
    pub on_block_images_change: Rc<dyn Fn(bool)>,
    pub on_enable_skill_commands_change: Rc<dyn Fn(bool)>,
    pub on_steering_mode_change: Rc<dyn Fn(SteeringMode)>,
    pub on_follow_up_mode_change: Rc<dyn Fn(FollowUpMode)>,
    pub on_transport_change: Rc<dyn Fn(Transport)>,
    pub on_http_idle_timeout_ms_change: Rc<dyn Fn(f64)>,
    pub on_thinking_level_change: Rc<dyn Fn(ThinkingLevel)>,
    pub on_theme_change: Rc<dyn Fn(&JsString)>,
    pub on_theme_preview: Option<ThemePreviewFn>,
    pub on_hide_thinking_block_change: Rc<dyn Fn(bool)>,
    pub on_hide_tool_output_change: Rc<dyn Fn(bool)>,
    pub on_hide_coordination_messages_change: Rc<dyn Fn(bool)>,
    pub on_collapse_changelog_change: Rc<dyn Fn(bool)>,
    pub on_enable_install_telemetry_change: Rc<dyn Fn(bool)>,
    pub on_double_escape_action_change: Rc<dyn Fn(crate::core::settings_manager::DoubleEscapeAction)>,
    pub on_tree_filter_mode_change: Rc<dyn Fn(TreeFilterMode)>,
    pub on_show_hardware_cursor_change: Rc<dyn Fn(bool)>,
    pub on_editor_padding_x_change: Rc<dyn Fn(f64)>,
    pub on_output_pad_change: Rc<dyn Fn(u8)>,
    pub on_autocomplete_max_visible_change: Rc<dyn Fn(f64)>,
    pub on_quiet_startup_change: Rc<dyn Fn(bool)>,
    pub on_default_project_trust_change: Rc<dyn Fn(DefaultProjectTrust)>,
    pub on_clear_on_shrink_change: Rc<dyn Fn(bool)>,
    pub on_show_terminal_progress_change: Rc<dyn Fn(bool)>,
    pub on_warnings_change: Rc<dyn Fn(Settings)>,
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
    container: Container,
    select_list: Rc<RefCell<SelectList>>,
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
        let select_list = Rc::new(RefCell::new(select_list));
        let mut children = vec![component(Text::new(
            theme.bold(theme.fg(ThemeColor::Accent, title)),
            0,
            0,
        ))];
        if !description.is_empty() {
            children.push(component(Spacer::new(1)));
            children.push(component(Text::new(theme.fg(ThemeColor::Muted, description), 0, 0)));
        }
        children.push(component(Spacer::new(1)));
        children.push(select_list.clone());
        children.push(component(Spacer::new(1)));
        children.push(component(Text::new(
            theme.fg(ThemeColor::Dim, "  Enter to select · Esc to go back"),
            0,
            0,
        )));
        Self {
            container: Container::with_children(children),
            select_list,
        }
    }

    pub fn select_list_mut(&mut self) -> RefMut<'_, SelectList> {
        self.select_list.borrow_mut()
    }
}

impl Component for SelectSubmenu {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.select_list.borrow_mut().handle_input(data);
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
}

/// Warning-settings submenu.
pub struct WarningSettingsSubmenu {
    container: Container,
    settings_list: Rc<RefCell<SettingsList>>,
}

impl WarningSettingsSubmenu {
    pub fn new(warnings: &Settings, on_change: Rc<dyn Fn(Settings)>, on_cancel: Rc<dyn Fn()>) -> Self {
        let state = RefCell::new(warnings.clone());
        let items = vec![SettingItem {
            id: "anthropic-extra-usage".into(),
            label: "Anthropic extra usage".into(),
            description: Some("Warn when Anthropic subscription auth may use paid extra usage".into()),
            current_value: bool_value(
                state
                    .borrow()
                    .get("anthropicExtraUsage")
                    .filter(|value| !matches!(value, JsValue::Null))
                    .is_none_or(pi_js_compat::js_json::is_truthy),
            )
            .into(),
            values: Some(bool_values()),
            submenu: None,
        }];
        let settings_list = SettingsList::new(
            items,
            items_len(1),
            get_settings_list_theme(),
            Rc::new(move |id: &JsString, new_value: &JsString| {
                if id == "anthropic-extra-usage" {
                    let value = {
                        let mut state = state.borrow_mut();
                        state.insert("anthropicExtraUsage".into(), JsValue::Bool(new_value == "true"));
                        state.clone()
                    };
                    on_change(value);
                }
            }),
            on_cancel,
            SettingsListOptions::default(),
        );
        let settings_list = Rc::new(RefCell::new(settings_list));
        Self {
            container: Container::with_children(vec![settings_list.clone()]),
            settings_list,
        }
    }
}

fn items_len(len: usize) -> usize {
    len.min(10)
}

impl Component for WarningSettingsSubmenu {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.settings_list.borrow_mut().handle_input(data);
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
}

/// Theme submenu with single/automatic modes.
pub struct ThemeSubmenu {
    container: Container,
    input: ComponentRef,
    state: Rc<RefCell<ThemeState>>,
    next_menu: Rc<Cell<Option<ThemeMode>>>,
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
        let automatic = default_automatic_themes(current_theme_setting, &available_themes);
        let fixed = if auto_theme.is_some() || current_theme_setting.contains('/') {
            None
        } else {
            Some(current_theme_setting)
        };
        let mode = if auto_theme.is_some() {
            ThemeMode::Automatic
        } else {
            ThemeMode::Single
        };
        let active = if terminal_theme == TerminalTheme::Light {
            &automatic.light_theme
        } else {
            &automatic.dark_theme
        };
        let single_theme = preferred_theme(&available_themes, fixed.or(auto_theme.as_ref().map(|_| active)), "dark");
        let mut submenu = Self {
            container: Container::new(),
            input: component(Container::new()),
            state: Rc::new(RefCell::new(ThemeState {
                mode,
                single_theme,
                light_theme: automatic.light_theme,
                dark_theme: automatic.dark_theme,
            })),
            next_menu: Rc::new(Cell::new(None)),
            callbacks,
            available_themes,
            terminal_theme,
            on_done,
            original_theme_setting: current_theme_setting.clone(),
        };
        submenu.show_menu(mode);
        submenu
    }

    fn cancel_callback(&self) -> Rc<dyn Fn()> {
        let callbacks = self.callbacks.clone();
        let original = self.original_theme_setting.clone();
        let done = self.on_done.clone();
        Rc::new(move || {
            if let Some(preview) = &callbacks.on_theme_preview {
                preview(&original);
            }
            done(None);
        })
    }

    fn set_content(&mut self, content: ComponentRef, input: ComponentRef) {
        self.container.clear();
        self.container.add_child(content);
        self.input = input;
    }

    fn show_menu(&mut self, mode: ThemeMode) {
        match mode {
            ThemeMode::Single => self.show_single_menu(),
            ThemeMode::Automatic => self.show_automatic_menu(),
        }
    }

    fn show_single_menu(&mut self) {
        self.state.borrow_mut().mode = ThemeMode::Single;
        let state = self.state.clone();
        let next = self.next_menu.clone();
        let callbacks = self.callbacks.clone();
        let done = self.on_done.clone();
        let preview_state = self.state.clone();
        let preview_callbacks = self.callbacks.clone();
        let menu = component(SelectSubmenu::new(
            "Theme",
            "Select a theme, or choose Automatic to follow terminal appearance.",
            single_mode_theme_items(&self.available_themes),
            &self.state.borrow().single_theme,
            Rc::new(move |value| {
                if value == AUTOMATIC_THEME_VALUE {
                    state.borrow_mut().mode = ThemeMode::Automatic;
                    let setting = theme_setting(&state.borrow());
                    if let Some(preview) = &callbacks.on_theme_preview {
                        preview(&setting);
                    }
                    next.set(Some(ThemeMode::Automatic));
                } else {
                    state.borrow_mut().single_theme = value.clone();
                    done(Some(value));
                }
            }),
            self.cancel_callback(),
            Some(Rc::new(move |value| {
                let setting = if value == AUTOMATIC_THEME_VALUE {
                    automatic_setting(&preview_state.borrow())
                } else {
                    value.clone()
                };
                if let Some(preview) = &preview_callbacks.on_theme_preview {
                    preview(&setting);
                }
            })),
        ));
        self.set_content(menu.clone(), menu);
    }

    fn theme_item(&self, light: bool) -> SettingItem {
        let state = self.state.clone();
        let callbacks = self.callbacks.clone();
        let themes = self.available_themes.clone();
        SettingItem {
            id: if light { "light-theme" } else { "dark-theme" }.into(),
            label: if light { "Light theme" } else { "Dark theme" }.into(),
            description: Some(
                if light {
                    "Theme to use in automatic mode when the terminal is light"
                } else {
                    "Theme to use in automatic mode when the terminal is dark"
                }
                .into(),
            ),
            current_value: if light {
                self.state.borrow().light_theme.clone()
            } else {
                self.state.borrow().dark_theme.clone()
            },
            values: None,
            submenu: Some(Rc::new(move |current, done| {
                let select_state = state.clone();
                let select_callbacks = callbacks.clone();
                let selected = done.clone();
                let cancel_state = state.clone();
                let cancel_callbacks = callbacks.clone();
                let preview_callbacks = callbacks.clone();
                component(SelectSubmenu::new(
                    if light { "Light Theme" } else { "Dark Theme" },
                    if light {
                        "Select the theme to use for light terminal appearance"
                    } else {
                        "Select the theme to use for dark terminal appearance"
                    },
                    theme_items(&themes),
                    current,
                    Rc::new(move |value| {
                        {
                            let mut state = select_state.borrow_mut();
                            if light {
                                state.light_theme = value.clone();
                            } else {
                                state.dark_theme = value.clone();
                            }
                        }
                        let setting = theme_setting(&select_state.borrow());
                        if let Some(preview) = &select_callbacks.on_theme_preview {
                            preview(&setting);
                        }
                        selected(Some(value.clone()));
                    }),
                    Rc::new(move || {
                        let setting = theme_setting(&cancel_state.borrow());
                        if let Some(preview) = &cancel_callbacks.on_theme_preview {
                            preview(&setting);
                        }
                        done(None);
                    }),
                    Some(Rc::new(move |value| {
                        if let Some(preview) = &preview_callbacks.on_theme_preview {
                            preview(value);
                        }
                    })),
                ))
            })),
        }
    }

    fn show_automatic_menu(&mut self) {
        self.state.borrow_mut().mode = ThemeMode::Automatic;
        let theme = crate::modes::interactive::theme::current_theme();
        let mut content = Container::with_children(vec![
            component(Text::new(
                theme.bold(theme.fg(ThemeColor::Accent, "Automatic Theme")),
                0,
                0,
            )),
            component(Spacer::new(1)),
            component(Text::new(
                theme.fg(
                    ThemeColor::Muted,
                    "Choose themes for terminal light and dark appearance.",
                ),
                0,
                0,
            )),
            component(Text::new(
                theme.fg(ThemeColor::Muted, "Light/dark detection requires terminal support."),
                0,
                0,
            )),
            component(Spacer::new(1)),
        ]);
        let items = vec![
            self.theme_item(true),
            self.theme_item(false),
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
        let state = self.state.clone();
        let next = self.next_menu.clone();
        let callbacks = self.callbacks.clone();
        let terminal = self.terminal_theme;
        let done = self.on_done.clone();
        let input = component(SettingsList::new(
            items,
            4,
            get_settings_list_theme(),
            Rc::new(move |id, _| {
                if id == "single-mode" {
                    let setting = {
                        let mut state = state.borrow_mut();
                        state.mode = ThemeMode::Single;
                        state.single_theme = active_automatic_theme(&state, terminal);
                        state.single_theme.clone()
                    };
                    if let Some(preview) = &callbacks.on_theme_preview {
                        preview(&setting);
                    }
                    next.set(Some(ThemeMode::Single));
                } else if id == "apply" {
                    let setting = automatic_setting(&state.borrow());
                    done(Some(&setting));
                }
            }),
            self.cancel_callback(),
            SettingsListOptions::default(),
        ));
        content.add_child(input.clone());
        self.set_content(component(content), input);
    }
}

impl Component for ThemeSubmenu {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }
    fn handle_input(&mut self, data: &JsString) {
        self.input.borrow_mut().handle_input(data);
        if let Some(mode) = self.next_menu.take() {
            self.show_menu(mode);
        }
    }
    fn invalidate(&mut self) {
        self.container.invalidate();
    }
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
}

/// Main settings selector.
pub struct SettingsSelectorComponent {
    container: Container,
    settings_list: Rc<RefCell<SettingsList>>,
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
                    current_value: js_number_to_string(config.image_width_cells).into(),
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
                current_value: js_number_to_string(config.editor_padding_x).into(),
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
                current_value: js_number_to_string(config.autocomplete_max_visible).into(),
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
        let settings_list = Rc::new(RefCell::new(settings_list));
        Self {
            container: Container::with_children(vec![
                component(dynamic_border()),
                settings_list.clone(),
                component(dynamic_border()),
            ]),
            settings_list,
        }
    }

    pub fn get_settings_list(&self) -> Ref<'_, SettingsList> {
        self.settings_list.borrow()
    }

    pub fn get_settings_list_mut(&mut self) -> RefMut<'_, SettingsList> {
        self.settings_list.borrow_mut()
    }
}

impl Component for SettingsSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.settings_list.borrow_mut().handle_input(data);
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }

    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
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
    current_warnings: Rc<RefCell<Settings>>,
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
            id: "hide-tool-output".into(),
            label: "Hide tool output".into(),
            description: Some("Hide tool results in the transcript; tool calls stay visible".into()),
            current_value: bool_value(config.hide_tool_output).into(),
            values: Some(bool_values()),
            submenu: None,
        },
        SettingItem {
            id: "hide-coordination-messages".into(),
            label: "Hide coordination messages".into(),
            description: Some("Hide mailbox notices and broadcasts in chat and queued previews; delivery stays unchanged".into()),
            current_value: bool_value(config.hide_coordination_messages).into(),
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
                    Rc::new({
                        let current_warnings = current_warnings.clone();
                        move |warnings| {
                            *current_warnings.borrow_mut() = warnings.clone();
                            on_change(warnings);
                        }
                    }),
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

macro_rules! inherit_container {
    ($($type:ty),+ $(,)?) => { $(
        impl Deref for $type {
            type Target = Container;
            fn deref(&self) -> &Container { &self.container }
        }
        impl DerefMut for $type {
            fn deref_mut(&mut self) -> &mut Container { &mut self.container }
        }
    )+ };
}
inherit_container!(
    SelectSubmenu,
    WarningSettingsSubmenu,
    ThemeSubmenu,
    SettingsSelectorComponent
);

fn build_change_handler(callbacks: Rc<SettingsCallbacks>) -> SettingsChangeHandler {
    Rc::new(move |id: &JsString, new_value: &JsString| {
        let boolean = new_value == "true";
        let Ok(id) = id.to_utf8() else {
            return;
        };
        match id.as_str() {
            "autocompact" => (callbacks.on_auto_compact_change)(boolean),
            "show-images" => (callbacks.on_show_images_change)(boolean),
            "image-width-cells" => (callbacks.on_image_width_cells_change)(parse_int(new_value.units(), 10)),
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
                    (callbacks.on_http_idle_timeout_ms_change)(*timeout_ms as f64);
                }
            }
            "hide-thinking" => (callbacks.on_hide_thinking_block_change)(boolean),
            "hide-tool-output" => (callbacks.on_hide_tool_output_change)(boolean),
            "hide-coordination-messages" => (callbacks.on_hide_coordination_messages_change)(boolean),
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
            "editor-padding" => (callbacks.on_editor_padding_x_change)(parse_int(new_value.units(), 10)),
            "output-padding" => (callbacks.on_output_pad_change)(if new_value == "0" { 0 } else { 1 }),
            "autocomplete-max-visible" => {
                (callbacks.on_autocomplete_max_visible_change)(parse_int(new_value.units(), 10));
            }
            "clear-on-shrink" => (callbacks.on_clear_on_shrink_change)(boolean),
            "terminal-progress" => (callbacks.on_show_terminal_progress_change)(boolean),
            "theme" => (callbacks.on_theme_change)(new_value),
            _ => {}
        }
    })
}
