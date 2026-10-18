//! Ported from `coding-agent/src/modes/interactive/components/extension-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use pi_tui::tui::component;
use pi_tui::{Component, Container, Spacer, Text};

use crate::modes::interactive::components::countdown_timer::CountdownTimer;
use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::{key_hint, raw_key_hint};

use crate::modes::interactive::theme::{Theme, ThemeColor};

/// Constructor options.
#[derive(Default)]
pub struct ExtensionSelectorOptions {
    /// `tui.requestRender` for the countdown redraw.
    pub request_render: Option<Rc<dyn Fn()>>,
    pub timeout: Option<f64>,
    pub on_toggle_tools_expanded: Option<Rc<dyn Fn()>>,
}

/// Generic string-option selector for extensions.
pub struct ExtensionSelectorComponent {
    options: Vec<String>,
    selected_index: usize,
    list_container: Container,
    on_select: Rc<dyn Fn(String)>,
    on_cancel: Rc<dyn Fn()>,
    title_text: Text,
    base_title: String,
    countdown: Option<CountdownTimer>,
    on_toggle_tools_expanded: Option<Rc<dyn Fn()>>,
    top: Container,
    after_title: Container,
    bottom: Container,
    theme: Arc<Theme>,
}

impl ExtensionSelectorComponent {
    pub fn new(
        title: &str,
        options: Vec<String>,
        on_select: Rc<dyn Fn(String)>,
        on_cancel: Rc<dyn Fn()>,
        opts: ExtensionSelectorOptions,
        theme: Arc<Theme>,
    ) -> Self {
        let mut top = Container::new();
        top.add_child(component(dynamic_border(&theme)));
        top.add_child(component(Spacer::new(1)));

        let title_text = Text::new(theme.fg(ThemeColor::Accent, &theme.bold(title)), 1, 0);

        let mut after_title = Container::new();
        after_title.add_child(component(Spacer::new(1)));

        let mut bottom = Container::new();
        bottom.add_child(component(Spacer::new(1)));
        let controls = raw_key_hint(&theme, "↑↓", "navigate")
            + "  "
            + &key_hint(&theme, "tui.select.confirm", "select")
            + "  "
            + &key_hint(&theme, "tui.select.cancel", "cancel");
        bottom.add_child(component(Text::new(controls, 1, 0)));
        bottom.add_child(component(Spacer::new(1)));
        bottom.add_child(component(dynamic_border(&theme)));

        let countdown = match (opts.timeout, opts.request_render) {
            (Some(timeout), Some(request_render)) if timeout > 0.0 => Some(CountdownTimer::new(
                timeout,
                Some(request_render),
                Rc::new(|_seconds: i64| {}),
                on_cancel.clone(),
            )),
            _ => None,
        };

        let mut selector = Self {
            options,
            selected_index: 0,
            list_container: Container::new(),
            on_select,
            on_cancel,
            title_text,
            base_title: title.to_string(),
            countdown,
            on_toggle_tools_expanded: opts.on_toggle_tools_expanded,
            top,
            after_title,
            bottom,
            theme,
        };
        selector.update_list();
        if selector.countdown.is_some() {
            selector.title_text.set_text(selector.countdown_title());
        }
        selector
    }

    fn update_list(&mut self) {
        self.list_container.clear();
        for (index, option) in self.options.iter().enumerate() {
            let is_selected = index == self.selected_index;
            let text = if is_selected {
                self.theme.fg(ThemeColor::Accent, "→ ") + &self.theme.fg(ThemeColor::Accent, option)
            } else {
                format!("  {}", self.theme.fg(ThemeColor::Text, option))
            };
            self.list_container.add_child(component(Text::new(text, 1, 0)));
        }
    }

    /// Drive the countdown timer; returns whether a tick fired.
    pub fn poll(&mut self, now: Instant) -> bool {
        match &mut self.countdown {
            Some(countdown) => {
                let ticked = countdown.poll(now);
                if ticked {
                    self.title_text.set_text(self.countdown_title());
                }
                ticked
            }
            None => false,
        }
    }

    fn countdown_title(&self) -> String {
        let seconds = self
            .countdown
            .as_ref()
            .map_or(0, |countdown| countdown.remaining_seconds());
        self.theme.fg(
            ThemeColor::Accent,
            &self.theme.bold(&format!("{} ({seconds}s)", self.base_title)),
        )
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn dispose(&mut self) {
        if let Some(countdown) = &mut self.countdown {
            countdown.dispose();
        }
    }
}

impl Component for ExtensionSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.title_text.render(width));
        lines.extend(self.after_title.render_children(width));
        lines.extend(self.list_container.render_children(width));
        lines.extend(self.bottom.render_children(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "app.tools.expand") {
            if let Some(on_toggle) = &self.on_toggle_tools_expanded {
                on_toggle();
            }
        } else if keybindings.matches(data, "tui.select.up") || data == "k" {
            self.selected_index = self.selected_index.saturating_sub(1);
            self.update_list();
        } else if keybindings.matches(data, "tui.select.down") || data == "j" {
            self.selected_index = (self.selected_index + 1).min(self.options.len().saturating_sub(1));
            self.update_list();
        } else if keybindings.matches(data, "tui.select.confirm") || data == "\n" {
            if let Some(selected) = self.options.get(self.selected_index) {
                (self.on_select)(selected.clone());
            }
        } else if keybindings.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        }
    }

    fn invalidate(&mut self) {
        self.top.invalidate_children();
        self.title_text.invalidate();
        self.after_title.invalidate_children();
        self.list_container.invalidate_children();
        self.bottom.invalidate_children();
    }
}
