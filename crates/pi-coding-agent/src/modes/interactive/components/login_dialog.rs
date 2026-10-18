//! Ported from `coding-agent/src/modes/interactive/components/login-dialog.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_ai::OAuthDeviceCodeInfo;
use pi_ai::auth::types::AuthInfoLink;
use pi_tui::components::input::InputOptions;
use pi_tui::tui::{ComponentRef, component};
use pi_tui::{Component, DynamicBorder, Focusable, Input, Spacer, Text};
use tokio_util::sync::CancellationToken;

use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};

/// TUI capability the dialog needs (`TUI.requestRender`).
pub trait LoginDialogUi {
    fn request_render(&self);
}

/// Rejection reason for a cancelled prompt (`new Error("Login cancelled")`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginError {
    pub message: String,
}

impl LoginError {
    fn cancelled() -> Self {
        Self {
            message: "Login cancelled".to_string(),
        }
    }
}

impl std::fmt::Display for LoginError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// Resolver for a pending prompt (`Promise<string>`).
pub type LoginInputResultFn = Rc<dyn Fn(Result<String, LoginError>)>;

/// Completion callback (`(success, message?) => void`).
pub type LoginCompleteFn = Rc<dyn Fn(bool, Option<&str>)>;

enum ContentChild {
    Component(ComponentRef),
    Input,
}

/// Login dialog replacing the editor during the OAuth flow.
pub struct LoginDialogComponent {
    content: Vec<ContentChild>,
    input: Input,
    ui: Rc<dyn LoginDialogUi>,
    abort: CancellationToken,
    input_resolver: Option<LoginInputResultFn>,
    on_complete: LoginCompleteFn,
    open_url: Rc<dyn Fn(&str)>,
    focused: bool,
    top_border: DynamicBorder,
    title: Text,
    bottom_border: DynamicBorder,
    theme: Arc<Theme>,
}

impl LoginDialogComponent {
    pub fn new(
        ui: Rc<dyn LoginDialogUi>,
        provider_id: &str,
        on_complete: LoginCompleteFn,
        provider_name_override: Option<&str>,
        title_override: Option<&str>,
        open_url: Rc<dyn Fn(&str)>,
        theme: Arc<Theme>,
    ) -> Self {
        let provider_name = provider_name_override.unwrap_or(provider_id);
        let title = title_override.map_or_else(|| format!("Login to {provider_name}"), str::to_string);

        let mut input = Input::new(InputOptions::default());
        input.on_escape = None;
        Self {
            content: Vec::new(),
            input,
            ui,
            abort: CancellationToken::new(),
            input_resolver: None,
            on_complete,
            open_url,
            focused: false,
            top_border: dynamic_border(&theme),
            title: Text::new(theme.fg(ThemeColor::Accent, &theme.bold(&title)), 1, 0),
            bottom_border: dynamic_border(&theme),
            theme,
        }
    }

    pub fn signal(&self) -> CancellationToken {
        self.abort.clone()
    }

    fn replace_input_with_submitted_text(&mut self, value: &str) {
        for child in &mut self.content {
            if matches!(child, ContentChild::Input) {
                *child = ContentChild::Component(component(Text::new(format!("> {value}"), 0, 0)));
            }
        }
    }

    fn cancel(&mut self) {
        self.abort.cancel();
        if let Some(resolver) = self.input_resolver.take() {
            resolver(Err(LoginError::cancelled()));
        }
        (self.on_complete)(false, Some("Login cancelled"));
    }

    fn push(&mut self, child: ComponentRef) {
        self.content.push(ContentChild::Component(child));
    }

    fn hyperlink(url: &str, text: &str) -> String {
        format!("\u{1b}]8;;{url}\u{7}{text}\u{1b}]8;;\u{7}")
    }

    fn click_hint() -> &'static str {
        if cfg!(target_os = "macos") {
            "Cmd+click to open"
        } else {
            "Ctrl+click to open"
        }
    }

    /// Called by `onAuth`: show URL and optional instructions.
    pub fn show_auth(&mut self, url: &str, instructions: Option<&str>) {
        self.content.clear();
        self.push(component(Spacer::new(1)));
        let linked_url = Self::hyperlink(url, url);
        self.push(component(Text::new(
            self.theme.fg(ThemeColor::Accent, &linked_url),
            1,
            0,
        )));
        let hyperlink = Self::hyperlink(url, Self::click_hint());
        self.push(component(Text::new(self.theme.fg(ThemeColor::Dim, &hyperlink), 1, 0)));
        if let Some(instructions) = instructions {
            self.push(component(Spacer::new(1)));
            self.push(component(Text::new(
                self.theme.fg(ThemeColor::Warning, instructions),
                1,
                0,
            )));
        }
        (self.open_url)(url);
        self.ui.request_render();
    }

    /// Called by `onDeviceCode`: show URL and user code.
    pub fn show_device_code(&mut self, info: &OAuthDeviceCodeInfo) {
        self.content.clear();
        self.push(component(Spacer::new(1)));
        let linked_url = Self::hyperlink(&info.verification_uri, &info.verification_uri);
        self.push(component(Text::new(
            self.theme.fg(ThemeColor::Accent, &linked_url),
            1,
            0,
        )));
        let hyperlink = Self::hyperlink(&info.verification_uri, Self::click_hint());
        self.push(component(Text::new(self.theme.fg(ThemeColor::Dim, &hyperlink), 1, 0)));
        self.push(component(Spacer::new(1)));
        self.push(component(Text::new(
            self.theme
                .fg(ThemeColor::Warning, &format!("Enter code: {}", info.user_code)),
            1,
            0,
        )));
        self.ui.request_render();
    }

    /// Show input for manual code/URL entry.
    pub fn show_manual_input(&mut self, prompt: &str, on_result: LoginInputResultFn) {
        self.input.set_value("");
        self.push(component(Spacer::new(1)));
        self.push(component(Text::new(self.theme.fg(ThemeColor::Dim, prompt), 1, 0)));
        self.content.push(ContentChild::Input);
        self.push(component(Text::new(
            format!("({})", key_hint(&self.theme, "tui.select.cancel", "to cancel")),
            1,
            0,
        )));
        self.ui.request_render();
        self.input_resolver = Some(on_result);
    }

    /// Called by `onPrompt`: show prompt and wait for input (appends).
    pub fn show_prompt(&mut self, message: &str, placeholder: Option<&str>, on_result: LoginInputResultFn) {
        self.push(component(Spacer::new(1)));
        self.push(component(Text::new(self.theme.fg(ThemeColor::Text, message), 1, 0)));
        if let Some(placeholder) = placeholder {
            self.push(component(Text::new(
                self.theme.fg(ThemeColor::Dim, &format!("e.g., {placeholder}")),
                1,
                0,
            )));
        }
        self.content.push(ContentChild::Input);
        self.push(component(Text::new(
            format!(
                "({} {})",
                key_hint(&self.theme, "tui.select.cancel", "to cancel,"),
                key_hint(&self.theme, "tui.select.confirm", "to submit")
            ),
            1,
            0,
        )));
        self.input.set_value("");
        self.ui.request_render();
        self.input_resolver = Some(on_result);
    }

    /// Show informational text before another login step.
    pub fn show_details(&mut self, lines: &[String]) {
        self.content.clear();
        self.push(component(Spacer::new(1)));
        for line in lines {
            self.push(component(Text::new(line.clone(), 1, 0)));
        }
        self.ui.request_render();
    }

    /// Show provider-owned information and links.
    pub fn show_info(&mut self, message: &str, links: &[AuthInfoLink], show_close_hint: bool) {
        self.push(component(Spacer::new(1)));
        self.push(component(Text::new(self.theme.fg(ThemeColor::Text, message), 1, 0)));
        for link in links {
            let text = match &link.label {
                Some(label) => format!("{label}: {}", link.url),
                None => link.url.clone(),
            };
            let hyperlink = Self::hyperlink(&link.url, &text);
            self.push(component(Text::new(
                self.theme.fg(ThemeColor::Accent, &hyperlink),
                1,
                0,
            )));
        }
        if show_close_hint {
            self.push(component(Spacer::new(1)));
            self.push(component(Text::new(
                format!("({})", key_hint(&self.theme, "tui.select.cancel", "to close")),
                1,
                0,
            )));
        }
        self.ui.request_render();
    }

    /// Show a waiting message (polling flows).
    pub fn show_waiting(&mut self, message: &str) {
        self.push(component(Spacer::new(1)));
        self.push(component(Text::new(self.theme.fg(ThemeColor::Dim, message), 1, 0)));
        self.push(component(Text::new(
            format!("({})", key_hint(&self.theme, "tui.select.cancel", "to cancel")),
            1,
            0,
        )));
        self.ui.request_render();
    }

    /// Called by `onProgress`.
    pub fn show_progress(&mut self, message: &str) {
        self.push(component(Text::new(self.theme.fg(ThemeColor::Dim, message), 1, 0)));
        self.ui.request_render();
    }
}

impl Focusable for LoginDialogComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.input.set_focused(focused);
    }
}

impl Component for LoginDialogComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top_border.render(width);
        lines.extend(self.title.render(width));
        for child in &mut self.content {
            match child {
                ContentChild::Component(component) => lines.extend(component.borrow_mut().render(width)),
                ContentChild::Input => lines.extend(self.input.render(width)),
            }
        }
        lines.extend(self.bottom_border.render(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "tui.select.cancel") {
            self.cancel();
            return;
        }
        if keybindings.matches(data, "tui.select.confirm")
            && let Some(resolver) = self.input_resolver.take()
        {
            let value = self.input.get_value().to_string();
            self.replace_input_with_submitted_text(&value);
            resolver(Ok(value));
            return;
        }
        self.input.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.title.invalidate();
        for child in &mut self.content {
            if let ContentChild::Component(component) = child {
                component.borrow_mut().invalidate();
            }
        }
        self.input.invalidate();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}
