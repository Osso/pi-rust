//! Ported from `packages/coding-agent/src/modes/interactive/components/oauth-selector.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::collections::HashSet;
use std::rc::Rc;

use pi_ai::auth::types::{ApiKeyAuth, AuthCheck, AuthType, OAuthAuth};
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::truncated_text::TruncatedText;
use pi_tui::fuzzy::fuzzy_filter;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, Focusable, component};

use super::super::theme::{ThemeColor, with_theme};
use super::dynamic_border::dynamic_border;

/// One selectable auth provider.
///
/// The TypeScript `method?: ApiKeyAuth | OAuthAuth` union is only read for its
/// `name` and `method?.login`, so the typed Rust carrier stores the resolved
/// name and the side-effect-free login availability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthSelectorProvider {
    pub id: String,
    pub name: String,
    pub auth_type: AuthType,
    pub method_name: Option<JsString>,
    pub login_supported: bool,
    pub status: Option<AuthCheck>,
}

impl AuthSelectorProvider {
    pub fn new(id: impl Into<String>, name: impl Into<String>, auth_type: AuthType) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            auth_type,
            method_name: None,
            login_supported: true,
            status: None,
        }
    }

    pub fn with_api_key_auth(mut self, auth: &dyn ApiKeyAuth) -> Self {
        self.method_name = Some(auth.name().into());
        self.login_supported = auth.supports_login();
        self
    }

    pub fn with_oauth_auth(mut self, auth: &dyn OAuthAuth) -> Self {
        self.method_name = Some(auth.name().into());
        self
    }
}

pub fn format_auth_selector_provider_type(auth_type: AuthType) -> &'static str {
    match auth_type {
        AuthType::Oauth => "subscription",
        AuthType::ApiKey => "API key",
    }
}

fn auth_type_query_value(auth_type: AuthType) -> &'static str {
    match auth_type {
        AuthType::Oauth => "oauth",
        AuthType::ApiKey => "api_key",
    }
}

fn format_status_indicator(provider: &AuthSelectorProvider) -> JsString {
    let Some(status) = &provider.status else {
        return with_theme(|theme| theme.fg(ThemeColor::Muted, " • unconfigured"));
    };
    if status.auth_type != provider.auth_type {
        let label = match status.auth_type {
            AuthType::Oauth => "subscription configured",
            AuthType::ApiKey => "API key configured",
        };
        return with_theme(|theme| theme.fg(ThemeColor::Muted, " • "))
            + &with_theme(|theme| theme.fg(ThemeColor::Warning, label));
    }
    let source = status.source.as_ref();
    if source.is_none_or(|source| source == "OAuth" || source == "stored credential") {
        return with_theme(|theme| theme.fg(ThemeColor::Success, " ✓ configured"));
    }
    let source = source.cloned().unwrap_or_default();
    let label = if is_env_source_list(&source) {
        js_format!("env: {}", source)
    } else {
        source.clone()
    };
    with_theme(|theme| theme.fg(ThemeColor::Success, &js_format!(" ✓ {}", label)))
}

/// `/^[A-Z][A-Z0-9_]*(?:, [A-Z][A-Z0-9_]*)*$/`
fn is_env_source_list(source: &JsString) -> bool {
    let is_upper = |unit: u16| (u16::from(b'A')..=u16::from(b'Z')).contains(&unit);
    let mut parts = source.units().split(|unit| *unit == u16::from(b','));
    let is_name = |units: &[u16]| {
        units.first().is_some_and(|unit| is_upper(*unit))
            && units.iter().all(|unit| {
                is_upper(*unit) || (u16::from(b'0')..=u16::from(b'9')).contains(unit) || *unit == u16::from(b'_')
            })
    };
    let Some(first) = parts.next() else {
        return false;
    };
    is_name(first) && parts.all(|part| part.first() == Some(&u16::from(b' ')) && is_name(&part[1..]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthSelectorMode {
    Login,
    Logout,
}

/// Auth provider selector with a fuzzy search input.
pub struct OAuthSelectorComponent {
    search_input: ComponentRef,
    list_container: Container,
    all_providers: Vec<AuthSelectorProvider>,
    filtered_providers: Vec<AuthSelectorProvider>,
    selected_index: usize,
    mode: AuthSelectorMode,
    on_select: Rc<dyn Fn(String, AuthType)>,
    on_cancel: Rc<dyn Fn()>,
    show_auth_type_labels: bool,
    search_value: JsString,
    focused: bool,
    prefix_children: Vec<ComponentRef>,
    suffix_children: Vec<ComponentRef>,
}

impl OAuthSelectorComponent {
    pub fn new(
        mode: AuthSelectorMode,
        providers: Vec<AuthSelectorProvider>,
        on_select: Rc<dyn Fn(String, AuthType)>,
        on_cancel: Rc<dyn Fn()>,
        initial_search_input: Option<&JsString>,
    ) -> Self {
        let show_auth_type_labels = providers
            .iter()
            .map(|provider| auth_type_query_value(provider.auth_type))
            .collect::<HashSet<_>>()
            .len()
            > 1;

        let title = match mode {
            AuthSelectorMode::Login => "Select provider to configure:",
            AuthSelectorMode::Logout => "Select provider to logout:",
        };
        let prefix_children = vec![
            component(dynamic_border()),
            component(Spacer::new(1)),
            component(TruncatedText::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, &theme.bold(title))),
                1,
                0,
            )),
            component(Spacer::new(1)),
        ];
        let suffix_children = vec![component(Spacer::new(1)), component(dynamic_border())];

        let mut input = Input::new(InputOptions::default());
        if let Some(initial) = initial_search_input {
            input.set_value(initial);
        }
        let search_input = component(input);

        let mut selector = Self {
            search_input,
            list_container: Container::new(),
            all_providers: providers,
            filtered_providers: Vec::new(),
            selected_index: 0,
            mode,
            on_select,
            on_cancel,
            show_auth_type_labels,
            search_value: initial_search_input.cloned().unwrap_or_default(),
            focused: false,
            prefix_children,
            suffix_children,
        };
        selector.filter_providers(&initial_search_input.cloned().unwrap_or_default());
        selector
    }

    fn filter_providers(&mut self, query: &JsString) {
        self.filtered_providers = if query.is_empty() {
            self.all_providers.clone()
        } else {
            fuzzy_filter(&self.all_providers, query, |provider| {
                vec![js_format!(
                    "{} {} {} {}",
                    provider.name,
                    provider.id,
                    auth_type_query_value(provider.auth_type),
                    provider.method_name.clone().unwrap_or_default()
                )]
            })
        };
        self.selected_index = self.selected_index.min(self.filtered_providers.len().saturating_sub(1));
        self.update_list();
    }

    fn update_list(&mut self) {
        self.list_container.clear();

        const MAX_VISIBLE: usize = 8;
        let length = self.filtered_providers.len();
        let start_index = self
            .selected_index
            .saturating_sub(MAX_VISIBLE / 2)
            .min(length.saturating_sub(MAX_VISIBLE));
        let end_index = (start_index + MAX_VISIBLE).min(length);

        for index in start_index..end_index {
            let provider = &self.filtered_providers[index];
            let is_selected = index == self.selected_index;
            let status_indicator = format_status_indicator(provider);
            let auth_type_label = if self.show_auth_type_labels {
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Muted,
                        &js_format!(" [{}]", format_auth_selector_provider_type(provider.auth_type)),
                    )
                })
            } else {
                JsString::default()
            };
            let line = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "→ "))
                    + &with_theme(|theme| theme.fg(ThemeColor::Accent, &provider.name))
                    + &auth_type_label
                    + &status_indicator
            } else {
                js_format!("  {}", with_theme(|theme| theme.fg(ThemeColor::Text, &provider.name)))
                    + &auth_type_label
                    + &status_indicator
            };
            self.list_container.add_child(component(TruncatedText::new(line, 1, 0)));
        }

        if start_index > 0 || end_index < length {
            let scroll_info = with_theme(|theme| {
                theme.fg(
                    ThemeColor::Muted,
                    &js_format!("  ({}/{})", self.selected_index + 1, length),
                )
            });
            self.list_container
                .add_child(component(TruncatedText::new(scroll_info, 1, 0)));
        }

        if self.filtered_providers.is_empty() {
            let message = if self.all_providers.is_empty() {
                match self.mode {
                    AuthSelectorMode::Login => "No providers available",
                    AuthSelectorMode::Logout => "No providers logged in. Use /login first.",
                }
            } else {
                "No matching providers"
            };
            let message = with_theme(|theme| theme.fg(ThemeColor::Muted, &js_format!("  {}", message)));
            self.list_container
                .add_child(component(TruncatedText::new(message, 1, 0)));
        }
    }

    pub fn get_search_input(&self) -> ComponentRef {
        Rc::clone(&self.search_input)
    }

    fn refresh_search_value(&mut self) {
        if let Some(input) = self
            .search_input
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Input>())
        {
            self.search_value = input.get_value().to_owned();
        }
    }
}

impl Component for OAuthSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = Vec::new();
        for child in &self.prefix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines.extend(self.search_input.borrow_mut().render(width));
        let mut spacer = Spacer::new(1);
        lines.extend(spacer.render(width));
        lines.extend(self.list_container.render(width));
        for child in &self.suffix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines
    }

    fn handle_input(&mut self, key_data: &JsString) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "tui.select.up") {
            if self.filtered_providers.is_empty() {
                return;
            }
            self.selected_index = self.selected_index.saturating_sub(1);
            self.update_list();
        } else if keybindings.matches(key_data, "tui.select.down") {
            if self.filtered_providers.is_empty() {
                return;
            }
            self.selected_index = (self.selected_index + 1).min(self.filtered_providers.len() - 1);
            self.update_list();
        } else if keybindings.matches(key_data, "tui.select.confirm") {
            if let Some(provider) = self.filtered_providers.get(self.selected_index) {
                (self.on_select)(provider.id.clone(), provider.auth_type);
            }
        } else if keybindings.matches(key_data, "tui.select.cancel") {
            (self.on_cancel)();
        } else {
            self.search_input.borrow_mut().handle_input(key_data);
            self.refresh_search_value();
            let query = self.search_value.clone();
            self.filter_providers(&query);
        }
    }

    fn invalidate(&mut self) {
        for child in &self.prefix_children {
            child.borrow_mut().invalidate();
        }
        self.search_input.borrow_mut().invalidate();
        self.list_container.invalidate();
        for child in &self.suffix_children {
            child.borrow_mut().invalidate();
        }
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}

impl Focusable for OAuthSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(input) = self
            .search_input
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Input>())
        {
            input.set_focused(focused);
        }
    }
}
