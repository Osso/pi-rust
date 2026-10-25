//! Ported from `packages/coding-agent/src/modes/interactive/components/oauth-selector.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::rc::Rc;

use crate::core::auth_storage::AuthStorage;
use crate::modes::interactive::login_options::LoginAuthType;
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::truncated_text::TruncatedText;
use pi_tui::fuzzy::fuzzy_filter;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, Focusable, component};

use super::super::theme::{ThemeColor, with_theme};
use super::dynamic_border::dynamic_border;

/// Frozen AuthSelectorProvider contains only id, name and authType.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthSelectorProvider {
    pub id: String,
    pub name: JsString,
    pub auth_type: LoginAuthType,
}

impl AuthSelectorProvider {
    pub fn new(id: impl Into<String>, name: impl Into<JsString>, auth_type: LoginAuthType) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            auth_type,
        }
    }
}

fn auth_type_query_value(auth_type: LoginAuthType) -> &'static str {
    match auth_type {
        LoginAuthType::Oauth => "oauth",
        LoginAuthType::ApiKey => "api_key",
    }
}

pub type GetAuthStatus = Rc<dyn Fn(&str) -> crate::core::auth_storage::AuthStatus>;

fn format_status_indicator(
    provider: &AuthSelectorProvider,
    auth_storage: &AuthStorage,
    get_auth_status: Option<&GetAuthStatus>,
) -> JsString {
    if let Some(credential) = auth_storage
        .get(provider.id.as_str())
        .filter(pi_js_compat::js_json::is_truthy)
    {
        let kind = credential
            .get("type")
            .and_then(pi_js_compat::js_json::JsValue::as_string);
        if kind.is_some_and(|kind| kind == auth_type_query_value(provider.auth_type)) {
            return with_theme(|theme| theme.fg(ThemeColor::Success, " ✓ configured"));
        }
        let label = if kind.is_some_and(|kind| kind == "oauth") {
            "subscription configured"
        } else {
            "API key configured"
        };
        return with_theme(|theme| theme.fg(ThemeColor::Muted, " • "))
            + &with_theme(|theme| theme.fg(ThemeColor::Warning, label));
    }
    let label = if provider.auth_type == LoginAuthType::ApiKey {
        let status = get_auth_status.map_or_else(
            || auth_storage.get_auth_status(&provider.id),
            |get_status| get_status(&provider.id),
        );
        match status.source.as_deref() {
            Some("environment") => Some(js_format!("env: {}", status.label.unwrap_or_else(|| "API key".into()))),
            Some("runtime") => Some("runtime API key".into()),
            Some("fallback") => Some("custom API key".into()),
            Some("models_json_key") => Some("key in models.json".into()),
            Some("models_json_command") => Some("command in models.json".into()),
            _ => None,
        }
    } else {
        None
    };
    match label {
        Some(label) => with_theme(|theme| theme.fg(ThemeColor::Success, &js_format!(" ✓ {}", label))),
        None => with_theme(|theme| theme.fg(ThemeColor::Muted, " • unconfigured")),
    }
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
    on_select: Rc<dyn Fn(String)>,
    on_cancel: Rc<dyn Fn()>,
    auth_storage: AuthStorage,
    get_auth_status: Option<GetAuthStatus>,
    search_value: JsString,
    focused: bool,
    prefix_children: Vec<ComponentRef>,
    suffix_children: Vec<ComponentRef>,
}

impl OAuthSelectorComponent {
    pub fn new(
        mode: AuthSelectorMode,
        auth_storage: AuthStorage,
        providers: Vec<AuthSelectorProvider>,
        on_select: Rc<dyn Fn(String)>,
        on_cancel: Rc<dyn Fn()>,
        get_auth_status: Option<GetAuthStatus>,
    ) -> Self {
        let title = match mode {
            AuthSelectorMode::Login => "Select provider to configure:",
            AuthSelectorMode::Logout => "Select provider to logout:",
        };
        let prefix_children = vec![
            component(dynamic_border()),
            component(Spacer::new(1)),
            component(TruncatedText::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, theme.bold(title))),
                1,
                0,
            )),
            component(Spacer::new(1)),
        ];
        let suffix_children = vec![component(Spacer::new(1)), component(dynamic_border())];

        let search_input = component(Input::new(InputOptions::default()));

        let mut selector = Self {
            search_input,
            list_container: Container::new(),
            all_providers: providers,
            filtered_providers: Vec::new(),
            selected_index: 0,
            mode,
            on_select,
            on_cancel,
            auth_storage,
            get_auth_status,
            search_value: JsString::default(),
            focused: false,
            prefix_children,
            suffix_children,
        };
        selector.filter_providers(&JsString::default());
        selector
    }

    fn filter_providers(&mut self, query: &JsString) {
        self.filtered_providers = if query.is_empty() {
            self.all_providers.clone()
        } else {
            fuzzy_filter(&self.all_providers, query, |provider| {
                vec![js_format!(
                    "{} {} {}",
                    provider.name,
                    provider.id,
                    auth_type_query_value(provider.auth_type)
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
            let status_indicator = format_status_indicator(provider, &self.auth_storage, self.get_auth_status.as_ref());
            let line = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "→ "))
                    + &with_theme(|theme| theme.fg(ThemeColor::Accent, &provider.name))
                    + &status_indicator
            } else {
                js_format!("  {}", with_theme(|theme| theme.fg(ThemeColor::Text, &provider.name))) + &status_indicator
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
                (self.on_select)(provider.id.clone());
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
