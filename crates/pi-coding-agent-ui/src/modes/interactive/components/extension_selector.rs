//! Ported from `packages/coding-agent/src/modes/interactive/components/extension-selector.ts`.

use std::rc::Rc;

use pi_js_compat::{js_format, js_json::JsString};

use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, component};

use super::super::theme::{ThemeColor, with_theme};
use super::countdown_timer::CountdownTimer;
use super::dynamic_border::dynamic_border;
use super::keybinding_hints::{key_hint, raw_key_hint};

/// `ExtensionSelectorOptions`.
#[derive(Default)]
pub struct ExtensionSelectorOptions {
    /// Request-render callback for the optional countdown (`opts.tui`).
    pub request_render: Option<Rc<dyn Fn()>>,
    pub timeout: Option<u64>,
    pub on_toggle_tools_expanded: Option<Rc<dyn Fn()>>,
}

/// Generic extension option list with keyboard navigation.
pub struct ExtensionSelectorComponent {
    options: Vec<JsString>,
    selected_index: usize,
    list_container: Container,
    on_select: Rc<dyn Fn(JsString)>,
    on_cancel: Rc<dyn Fn()>,
    title_text: ComponentRef,
    base_title: JsString,
    countdown: Option<CountdownTimer>,
    on_toggle_tools_expanded: Option<Rc<dyn Fn()>>,
    prefix_children: Vec<ComponentRef>,
    suffix_children: Vec<ComponentRef>,
}

impl ExtensionSelectorComponent {
    pub fn new(
        title: &JsString,
        options: Vec<JsString>,
        on_select: Rc<dyn Fn(JsString)>,
        on_cancel: Rc<dyn Fn()>,
        options_config: ExtensionSelectorOptions,
    ) -> Self {
        let base_title = title.to_owned();
        let title_text = component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Accent, theme.bold(title))),
            1,
            0,
        ));

        let mut countdown = None;
        if let Some(timeout) = options_config.timeout.filter(|timeout| *timeout > 0)
            && let Some(request_render) = options_config.request_render.clone()
        {
            let tick_title = Rc::clone(&title_text);
            let tick_base_title = base_title.clone();
            let tick = Rc::new(move |seconds: f64| {
                set_text(
                    &tick_title,
                    with_theme(|theme| {
                        theme.fg(
                            ThemeColor::Accent,
                            theme.bold(js_format!(
                                "{} ({}s)",
                                tick_base_title,
                                pi_js_compat::number::js_number_to_string(seconds)
                            )),
                        )
                    }),
                );
            });
            let expire = Rc::clone(&on_cancel);
            countdown = Some(CountdownTimer::new(timeout as f64, Some(request_render), tick, expire));
        }

        let prefix_children = vec![
            component(dynamic_border()),
            component(Spacer::new(1)),
            Rc::clone(&title_text),
            component(Spacer::new(1)),
        ];
        let suffix_children = vec![
            component(Spacer::new(1)),
            component(Text::new(
                raw_key_hint("↑↓", "navigate")
                    + "  "
                    + &key_hint("tui.select.confirm", "select")
                    + "  "
                    + &key_hint("tui.select.cancel", "cancel"),
                1,
                0,
            )),
            component(Spacer::new(1)),
            component(dynamic_border()),
        ];

        let mut selector = Self {
            options,
            selected_index: 0,
            list_container: Container::new(),
            on_select,
            on_cancel,
            title_text,
            base_title,
            countdown,
            on_toggle_tools_expanded: options_config.on_toggle_tools_expanded,
            prefix_children,
            suffix_children,
        };
        selector.update_list();
        selector
    }

    fn update_list(&mut self) {
        self.list_container.clear();
        for (index, option) in self.options.iter().enumerate() {
            let is_selected = index == self.selected_index;
            let text = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "→ "))
                    + &with_theme(|theme| theme.fg(ThemeColor::Accent, option))
            } else {
                JsString::from("  ") + with_theme(|theme| theme.fg(ThemeColor::Text, option))
            };
            self.list_container.add_child(component(Text::new(text, 1, 0)));
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

impl Component for ExtensionSelectorComponent {
    fn dispose(&mut self) {
        ExtensionSelectorComponent::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = Vec::new();
        for child in &self.prefix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines.extend(self.list_container.render(width));
        for child in &self.suffix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines
    }

    fn handle_input(&mut self, key_data: &JsString) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "app.tools.expand") {
            if let Some(toggle) = &self.on_toggle_tools_expanded {
                toggle();
            }
        } else if keybindings.matches(key_data, "tui.select.up") || key_data == "k" {
            self.selected_index = self.selected_index.saturating_sub(1);
            self.update_list();
        } else if keybindings.matches(key_data, "tui.select.down") || key_data == "j" {
            self.selected_index = (self.selected_index + 1).min(self.options.len() - 1);
            self.update_list();
        } else if keybindings.matches(key_data, "tui.select.confirm") || key_data == "\n" {
            if let Some(selected) = self.options.get(self.selected_index) {
                (self.on_select)(selected.clone());
            }
        } else if keybindings.matches(key_data, "tui.select.cancel") {
            (self.on_cancel)();
        }
    }

    fn invalidate(&mut self) {
        for child in &self.prefix_children {
            child.borrow_mut().invalidate();
        }
        self.list_container.invalidate();
        for child in &self.suffix_children {
            child.borrow_mut().invalidate();
        }
    }
}
