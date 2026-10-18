//! Ported from `coding-agent/src/modes/interactive/components/oauth-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_ai::auth::types::{AuthCheck, AuthType};
use pi_tui::components::input::InputOptions;
use pi_tui::tui::component;
use pi_tui::{Component, Container, Focusable, Input, Spacer, TruncatedText, fuzzy_filter};

use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::theme::{Theme, ThemeColor};

/// `ApiKeyAuth | OAuthAuth` as used by the selector (only `name` is read).
#[derive(Clone)]
pub enum AuthSelectorMethod {
    ApiKey(Arc<dyn pi_ai::auth::types::ApiKeyAuth>),
    OAuth(Arc<dyn pi_ai::auth::types::OAuthAuth>),
}

impl AuthSelectorMethod {
    fn name(&self) -> &str {
        match self {
            Self::ApiKey(auth) => auth.name(),
            Self::OAuth(auth) => auth.name(),
        }
    }
}

/// One selectable auth provider.
#[derive(Clone)]
pub struct AuthSelectorProvider {
    pub id: String,
    pub name: String,
    pub auth_type: AuthType,
    pub method: Option<AuthSelectorMethod>,
    pub status: Option<AuthCheck>,
}

/// `formatAuthSelectorProviderType`.
pub fn format_auth_selector_provider_type(auth_type: AuthType) -> &'static str {
    match auth_type {
        AuthType::Oauth => "subscription",
        AuthType::ApiKey => "API key",
    }
}

/// Login/logout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthSelectorMode {
    Login,
    Logout,
}

/// Auth provider selector with fuzzy search.
pub struct OAuthSelectorComponent {
    search_input: Input,
    list_container: Container,
    all_providers: Vec<AuthSelectorProvider>,
    filtered_providers: Vec<AuthSelectorProvider>,
    selected_index: usize,
    mode: AuthSelectorMode,
    on_select: Rc<dyn Fn(&str, AuthType)>,
    on_cancel: Rc<dyn Fn()>,
    show_auth_type_labels: bool,
    focused: bool,
    top: Container,
    bottom: Container,
    theme: Arc<Theme>,
}

impl OAuthSelectorComponent {
    pub fn new(
        mode: AuthSelectorMode,
        providers: Vec<AuthSelectorProvider>,
        on_select: Rc<dyn Fn(&str, AuthType)>,
        on_cancel: Rc<dyn Fn()>,
        initial_search_input: Option<&str>,
        theme: Arc<Theme>,
    ) -> Self {
        let mut auth_types: Vec<AuthType> = Vec::new();
        for provider in &providers {
            if !auth_types.contains(&provider.auth_type) {
                auth_types.push(provider.auth_type);
            }
        }
        let show_auth_type_labels = auth_types.len() > 1;
        let filtered_providers = providers.clone();

        let mut top = Container::new();
        top.add_child(component(dynamic_border(&theme)));
        top.add_child(component(Spacer::new(1)));
        let title = match mode {
            AuthSelectorMode::Login => "Select provider to configure:",
            AuthSelectorMode::Logout => "Select provider to logout:",
        };
        top.add_child(component(TruncatedText::new(
            theme.fg(ThemeColor::Accent, &theme.bold(title)),
            1,
            0,
        )));
        let mut search_input = Input::new(InputOptions::default());
        if let Some(initial) = initial_search_input {
            search_input.set_value(initial);
        }
        top.add_child(component(Spacer::new(1)));

        let mut bottom = Container::new();
        bottom.add_child(component(Spacer::new(1)));
        bottom.add_child(component(dynamic_border(&theme)));

        let mut selector = Self {
            search_input,
            list_container: Container::new(),
            all_providers: providers,
            filtered_providers,
            selected_index: 0,
            mode,
            on_select,
            on_cancel,
            show_auth_type_labels,
            focused: false,
            top,
            bottom,
            theme,
        };
        selector.filter_providers(initial_search_input.unwrap_or(""));
        selector
    }

    fn filter_providers(&mut self, query: &str) {
        self.filtered_providers = if query.is_empty() {
            self.all_providers.clone()
        } else {
            fuzzy_filter(&self.all_providers, query, |provider| {
                vec![format!(
                    "{} {} {} {}",
                    provider.name,
                    provider.id,
                    provider.auth_type.as_str(),
                    provider.method.as_ref().map_or("", |method| method.name())
                )]
            })
        };
        let max_index = self.filtered_providers.len().saturating_sub(1);
        self.selected_index = self.selected_index.min(max_index);
        self.update_list();
    }

    fn update_list(&mut self) {
        self.list_container.clear();

        let max_visible = 8usize;
        let start_index = (self.selected_index as i64 - (max_visible / 2) as i64)
            .min(self.filtered_providers.len() as i64 - max_visible as i64)
            .max(0) as usize;
        let end_index = (start_index + max_visible).min(self.filtered_providers.len());

        for index in start_index..end_index {
            let provider = &self.filtered_providers[index];
            let is_selected = index == self.selected_index;
            let status_indicator = self.format_status_indicator(provider);
            let auth_type_label = if self.show_auth_type_labels {
                self.theme.fg(
                    ThemeColor::Muted,
                    &format!(" [{}]", format_auth_selector_provider_type(provider.auth_type)),
                )
            } else {
                String::new()
            };
            let line = if is_selected {
                self.theme.fg(ThemeColor::Accent, "→ ")
                    + &self.theme.fg(ThemeColor::Accent, &provider.name)
                    + &auth_type_label
                    + &status_indicator
            } else {
                format!("  {}", self.theme.fg(ThemeColor::Text, &provider.name)) + &auth_type_label + &status_indicator
            };
            self.list_container.add_child(component(TruncatedText::new(line, 1, 0)));
        }

        if start_index > 0 || end_index < self.filtered_providers.len() {
            let scroll_info = self.theme.fg(
                ThemeColor::Muted,
                &format!("  ({}/{})", self.selected_index + 1, self.filtered_providers.len()),
            );
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
            self.list_container.add_child(component(TruncatedText::new(
                self.theme.fg(ThemeColor::Muted, &format!("  {message}")),
                1,
                0,
            )));
        }
    }

    fn format_status_indicator(&self, provider: &AuthSelectorProvider) -> String {
        let Some(status) = &provider.status else {
            return self.theme.fg(ThemeColor::Muted, " • unconfigured");
        };
        if status.auth_type != provider.auth_type {
            let label = match status.auth_type {
                AuthType::Oauth => "subscription configured",
                AuthType::ApiKey => "API key configured",
            };
            return self.theme.fg(ThemeColor::Muted, " • ") + &self.theme.fg(ThemeColor::Warning, label);
        }
        let Some(source) = &status.source else {
            return self.theme.fg(ThemeColor::Success, " ✓ configured");
        };
        if source == "OAuth" || source == "stored credential" {
            return self.theme.fg(ThemeColor::Success, " ✓ configured");
        }
        let source = if is_env_var_list(source) {
            format!("env: {source}")
        } else {
            source.clone()
        };
        self.theme.fg(ThemeColor::Success, &format!(" ✓ {source}"))
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn filtered_providers(&self) -> &[AuthSelectorProvider] {
        &self.filtered_providers
    }

    pub fn search_input_mut(&mut self) -> &mut Input {
        &mut self.search_input
    }
}

/// `/^[A-Z][A-Z0-9_]*(?:, [A-Z][A-Z0-9_]*)*$/`
fn is_env_var_list(source: &str) -> bool {
    source.split(", ").all(|name| {
        let mut characters = name.chars();
        characters.next().is_some_and(|first| first.is_ascii_uppercase())
            && characters
                .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_')
    }) && !source.is_empty()
}

impl Focusable for OAuthSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.search_input.set_focused(focused);
    }
}

impl Component for OAuthSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.search_input.render(width));
        lines.extend(Spacer::new(1).render(width));
        lines.extend(self.list_container.render_children(width));
        lines.extend(self.bottom.render_children(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "tui.select.up") {
            if self.filtered_providers.is_empty() {
                return;
            }
            self.selected_index = self.selected_index.saturating_sub(1);
            self.update_list();
        } else if keybindings.matches(data, "tui.select.down") {
            if self.filtered_providers.is_empty() {
                return;
            }
            self.selected_index = (self.selected_index + 1).min(self.filtered_providers.len() - 1);
            self.update_list();
        } else if keybindings.matches(data, "tui.select.confirm") {
            if let Some(selected) = self.filtered_providers.get(self.selected_index) {
                (self.on_select)(&selected.id, selected.auth_type);
            }
        } else if keybindings.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        } else {
            self.search_input.handle_input(data);
            let query = self.search_input.get_value().to_string();
            self.filter_providers(&query);
        }
    }

    fn invalidate(&mut self) {
        self.top.invalidate_children();
        self.search_input.invalidate();
        self.list_container.invalidate_children();
        self.bottom.invalidate_children();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}
