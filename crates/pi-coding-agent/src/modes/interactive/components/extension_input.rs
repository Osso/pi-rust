//! Ported from `coding-agent/src/modes/interactive/components/extension-input.ts`.

use std::rc::Rc;

use pi_js_compat::{js_format, js_json::JsString};

use pi_tui::components::input::{Input, InputOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, Focusable, component};

use super::dynamic_border::dynamic_border;
use crate::modes::interactive::components::countdown_timer::CountdownTimer;
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{ThemeColor, with_theme};

/// `ExtensionInputOptions`.
#[derive(Default)]
pub struct ExtensionInputOptions {
    /// Request-render callback for the optional countdown (`opts.tui`).
    pub request_render: Option<Rc<dyn Fn()>>,
    pub timeout: Option<u64>,
    pub secret: bool,
}

/// Simple text input component for extensions.
pub struct ExtensionInputComponent {
    container: Container,
    input: ComponentRef,
    title_text: ComponentRef,
    base_title: JsString,
    countdown: Option<CountdownTimer>,
    on_submit: Rc<dyn Fn(JsString)>,
    on_cancel: Rc<dyn Fn()>,
    focused: bool,
}

impl ExtensionInputComponent {
    pub fn new(
        title: &JsString,
        _placeholder: Option<&JsString>,
        on_submit: Rc<dyn Fn(JsString)>,
        on_cancel: Rc<dyn Fn()>,
        options: ExtensionInputOptions,
    ) -> Self {
        let mut container = Container::new();
        container.add_child(component(dynamic_border()));
        container.add_child(component(Spacer::new(1)));

        let title_text = component(Text::new(with_theme(|theme| theme.fg(ThemeColor::Accent, title)), 1, 0));
        container.add_child(Rc::clone(&title_text));
        container.add_child(component(Spacer::new(1)));

        let mut countdown = None;
        if let Some(timeout) = options.timeout.filter(|timeout| *timeout > 0)
            && let Some(request_render) = options.request_render.clone()
        {
            let tick_title = Rc::clone(&title_text);
            let base_title = title.to_owned();
            let tick = Rc::new(move |seconds: i64| {
                set_text(
                    &tick_title,
                    with_theme(|theme| {
                        theme.fg(
                            ThemeColor::Accent,
                            js_format!("{} ({}s)", base_title, seconds.to_string()),
                        )
                    }),
                );
            });
            let expire = Rc::clone(&on_cancel);
            countdown = Some(CountdownTimer::new(timeout, Some(request_render), tick, expire));
        }

        let input = component(Input::new(InputOptions { mask: options.secret }));
        container.add_child(Rc::clone(&input));
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(Text::new(
            js_format!(
                "{}  {}",
                key_hint("tui.select.confirm", "submit"),
                key_hint("tui.select.cancel", "cancel")
            ),
            1,
            0,
        )));
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(dynamic_border()));

        Self {
            container,
            input,
            title_text,
            base_title: title.to_owned(),
            countdown,
            on_submit,
            on_cancel,
            focused: false,
        }
    }

    pub fn dispose(&mut self) {
        if let Some(countdown) = &mut self.countdown {
            countdown.dispose();
        }
    }

    pub fn poll_countdown(&mut self, now: std::time::Instant) -> bool {
        self.countdown.as_mut().is_some_and(|countdown| countdown.poll(now))
    }

    pub fn next_countdown_deadline(&self) -> Option<std::time::Instant> {
        self.countdown.as_ref().and_then(CountdownTimer::next_deadline)
    }

    pub fn title_text(&self) -> ComponentRef {
        Rc::clone(&self.title_text)
    }

    pub fn base_title(&self) -> &JsString {
        &self.base_title
    }

    fn input_value(&mut self) -> JsString {
        self.input
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Input>())
            .map(|input| input.get_value().to_owned())
            .unwrap_or_default()
    }
}

fn set_text(target: &ComponentRef, text: JsString) {
    if let Some(text_component) = target
        .borrow_mut()
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<Text>())
    {
        text_component.set_text(text);
    }
}

impl Component for ExtensionInputComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.confirm") || data == "\n" {
            let value = self.input_value();
            (self.on_submit)(value);
        } else if keybindings.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        } else {
            self.input.borrow_mut().handle_input(data);
        }
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}

impl Focusable for ExtensionInputComponent {
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
