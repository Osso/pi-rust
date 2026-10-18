//! Ported from `packages/coding-agent/src/modes/interactive/components/login-dialog.ts`.
//!
//! The TypeScript `Promise` returned by `showManualInput`/`showPrompt` is a
//! `tokio::sync::oneshot` receiver resolved by the input submit handler; the
//! `AbortSignal` is a `CancellationToken`.

use std::cell::RefCell;
use std::rc::Rc;

use pi_ai::OAuthDeviceCodeInfo;
use pi_ai::auth::types::AuthInfoLink;
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Focusable, component};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::super::theme::{ThemeColor, with_theme};
use super::dynamic_border::dynamic_border;
use super::keybinding_hints::key_hint;
use crate::utils::open_browser::open_browser;

/// Rejection value for a cancelled prompt (`new Error("Login cancelled")`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginError(pub String);

impl std::fmt::Display for LoginError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for LoginError {}

type PromptReceiver = oneshot::Receiver<Result<String, LoginError>>;

struct LoginDialogCore {
    content: Vec<ComponentRef>,
    input: ComponentRef,
    pending: Option<oneshot::Sender<Result<String, LoginError>>>,
    cancellation: CancellationToken,
    on_complete: Rc<dyn Fn(bool, Option<String>)>,
    request_render: Rc<dyn Fn()>,
}

impl LoginDialogCore {
    fn replace_input_with_submitted_text(&mut self, value: &str) {
        self.content = self
            .content
            .iter()
            .map(|child| {
                if Rc::ptr_eq(child, &self.input) {
                    component(Text::new(format!("> {value}"), 0, 0))
                } else {
                    Rc::clone(child)
                }
            })
            .collect();
    }

    fn add(&mut self, child: ComponentRef) {
        self.content.push(child);
    }

    fn cancel(&mut self) {
        self.cancellation.cancel();
        if let Some(sender) = self.pending.take() {
            let _ = sender.send(Err(LoginError("Login cancelled".to_owned())));
        }
        (self.on_complete)(false, Some("Login cancelled".to_owned()));
    }

    fn request_render(&self) {
        (self.request_render)();
    }
}

/// Login dialog shown in place of the editor during an auth flow.
pub struct LoginDialogComponent {
    core: Rc<RefCell<LoginDialogCore>>,
    input: ComponentRef,
    prefix_children: Vec<ComponentRef>,
    suffix_children: Vec<ComponentRef>,
    focused: bool,
}

fn click_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+click to open"
    } else {
        "Ctrl+click to open"
    }
}

impl LoginDialogComponent {
    pub fn new(
        request_render: Rc<dyn Fn()>,
        provider_id: &str,
        on_complete: Rc<dyn Fn(bool, Option<String>)>,
        provider_name_override: Option<&str>,
        title_override: Option<&str>,
    ) -> Self {
        let provider_name = provider_name_override
            .filter(|name| !name.is_empty())
            .unwrap_or(provider_id);
        let title = title_override
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Login to {provider_name}"));

        let prefix_children = vec![
            component(dynamic_border()),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, &theme.bold(&title))),
                1,
                0,
            )),
        ];
        let suffix_children = vec![component(dynamic_border())];

        let input = component(Input::new(InputOptions::default()));
        let core = Rc::new(RefCell::new(LoginDialogCore {
            content: Vec::new(),
            input: Rc::clone(&input),
            pending: None,
            cancellation: CancellationToken::new(),
            on_complete,
            request_render,
        }));

        {
            let submit_core = Rc::clone(&core);
            let escape_core = Rc::clone(&core);
            if let Some(input_component) = input
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<Input>())
            {
                input_component.on_submit = Some(Rc::new(move |value: &str| {
                    let mut core = submit_core.borrow_mut();
                    core.replace_input_with_submitted_text(value);
                    if let Some(sender) = core.pending.take() {
                        let _ = sender.send(Ok(value.to_owned()));
                    }
                }));
                input_component.on_escape = Some(Rc::new(move || {
                    escape_core.borrow_mut().cancel();
                }));
            }
        }

        Self {
            core,
            input,
            prefix_children,
            suffix_children,
            focused: false,
        }
    }

    pub fn signal(&self) -> CancellationToken {
        self.core.borrow().cancellation.clone()
    }

    fn clear_content(&self) {
        self.core.borrow_mut().content.clear();
    }

    fn add_content(&self, child: ComponentRef) {
        self.core.borrow_mut().add(child);
    }

    /// Show the authorization URL and optional instructions.
    ///
    /// Opens the URL in the browser like the source; the content is testable
    /// through [`LoginDialogComponent::render`] only when the browser launch is
    /// acceptable to the caller.
    pub fn show_auth(&self, url: &str, instructions: Option<&str>) {
        self.clear_content();
        self.add_content(component(Spacer::new(1)));
        let linked_url = format!("\x1b]8;;{url}\x07{url}\x1b]8;;\x07");
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Accent, &linked_url)),
            1,
            0,
        )));
        let hyperlink = format!("\x1b]8;;{url}\x07{}\x1b]8;;\x07", click_hint());
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Dim, &hyperlink)),
            1,
            0,
        )));
        if let Some(instructions) = instructions {
            self.add_content(component(Spacer::new(1)));
            self.add_content(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Warning, instructions)),
                1,
                0,
            )));
        }
        open_browser(url);
        self.core.borrow().request_render();
    }

    /// Show the device-code verification URL and user code.
    pub fn show_device_code(&self, info: &OAuthDeviceCodeInfo) {
        self.clear_content();
        self.add_content(component(Spacer::new(1)));
        let url = &info.verification_uri;
        let linked_url = format!("\x1b]8;;{url}\x07{url}\x1b]8;;\x07");
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Accent, &linked_url)),
            1,
            0,
        )));
        let hyperlink = format!("\x1b]8;;{url}\x07{}\x1b]8;;\x07", click_hint());
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Dim, &hyperlink)),
            1,
            0,
        )));
        self.add_content(component(Spacer::new(1)));
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Warning, &format!("Enter code: {}", info.user_code))),
            1,
            0,
        )));
        self.core.borrow().request_render();
    }

    fn begin_prompt(&self) -> PromptReceiver {
        let (sender, receiver) = oneshot::channel();
        self.core.borrow_mut().pending = Some(sender);
        receiver
    }

    /// Show an input for manual code/URL entry.
    pub fn show_manual_input(&self, prompt: &str) -> PromptReceiver {
        if let Some(input) = self
            .input
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Input>())
        {
            input.set_value("");
        }
        self.add_content(component(Spacer::new(1)));
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Dim, prompt)),
            1,
            0,
        )));
        self.add_content(Rc::clone(&self.input));
        self.add_content(component(Text::new(
            format!("({})", key_hint("tui.select.cancel", "to cancel")),
            1,
            0,
        )));
        self.core.borrow().request_render();
        self.begin_prompt()
    }

    /// Show a prompt and wait for input, appending to the current content.
    pub fn show_prompt(&self, message: &str, placeholder: Option<&str>) -> PromptReceiver {
        self.add_content(component(Spacer::new(1)));
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Text, message)),
            1,
            0,
        )));
        if let Some(placeholder) = placeholder {
            self.add_content(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("e.g., {placeholder}"))),
                1,
                0,
            )));
        }
        self.add_content(Rc::clone(&self.input));
        self.add_content(component(Text::new(
            format!(
                "({} {})",
                key_hint("tui.select.cancel", "to cancel,"),
                key_hint("tui.select.confirm", "to submit")
            ),
            1,
            0,
        )));
        if let Some(input) = self
            .input
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Input>())
        {
            input.set_value("");
        }
        self.core.borrow().request_render();
        self.begin_prompt()
    }

    /// Show informational text before another login step.
    pub fn show_details(&self, lines: &[String]) {
        self.clear_content();
        self.add_content(component(Spacer::new(1)));
        for line in lines {
            self.add_content(component(Text::new(line.clone(), 1, 0)));
        }
        self.core.borrow().request_render();
    }

    /// Show provider-owned information and links.
    pub fn show_info(&self, message: &str, links: &[AuthInfoLink], show_close_hint: bool) {
        self.add_content(component(Spacer::new(1)));
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Text, message)),
            1,
            0,
        )));
        for link in links {
            let text = match &link.label {
                Some(label) => format!("{label}: {}", link.url),
                None => link.url.clone(),
            };
            let hyperlink = format!("\x1b]8;;{}\x07{text}\x1b]8;;\x07", link.url);
            self.add_content(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, &hyperlink)),
                1,
                0,
            )));
        }
        if show_close_hint {
            self.add_content(component(Spacer::new(1)));
            self.add_content(component(Text::new(
                format!("({})", key_hint("tui.select.cancel", "to close")),
                1,
                0,
            )));
        }
        self.core.borrow().request_render();
    }

    /// Show a waiting message for polling flows.
    pub fn show_waiting(&self, message: &str) {
        self.add_content(component(Spacer::new(1)));
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Dim, message)),
            1,
            0,
        )));
        self.add_content(component(Text::new(
            format!("({})", key_hint("tui.select.cancel", "to cancel")),
            1,
            0,
        )));
        self.core.borrow().request_render();
    }

    /// Show progress text.
    pub fn show_progress(&self, message: &str) {
        self.add_content(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Dim, message)),
            1,
            0,
        )));
        self.core.borrow().request_render();
    }

    pub fn get_input(&self) -> ComponentRef {
        Rc::clone(&self.input)
    }
}

impl Component for LoginDialogComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for child in &self.prefix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        let content = self.core.borrow().content.clone();
        for child in content {
            lines.extend(child.borrow_mut().render(width));
        }
        for child in &self.suffix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.cancel") {
            self.core.borrow_mut().cancel();
            return;
        }
        self.input.borrow_mut().handle_input(data);
    }

    fn invalidate(&mut self) {
        for child in &self.prefix_children {
            child.borrow_mut().invalidate();
        }
        let content = self.core.borrow().content.clone();
        for child in content {
            child.borrow_mut().invalidate();
        }
        for child in &self.suffix_children {
            child.borrow_mut().invalidate();
        }
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}

impl Focusable for LoginDialogComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(input) = self
            .input
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Input>())
        {
            input.set_focused(focused);
        }
    }
}
