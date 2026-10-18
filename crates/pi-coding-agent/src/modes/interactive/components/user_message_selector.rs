//! Ported from `coding-agent/src/modes/interactive/components/user-message-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_js_compat::string::js_trim;
use pi_tui::tui::component;
use pi_tui::{Component, Container, Spacer, Text, truncate_to_width};

use crate::modes::interactive::components::dynamic_border::dynamic_border;

use crate::modes::interactive::theme::{Theme, ThemeColor};

const MAX_VISIBLE: usize = 10;
const AUTO_CANCEL_DELAY: Duration = Duration::from_millis(100);

/// One user message offered for branching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMessageItem {
    pub id: String,
    pub text: String,
    pub timestamp: Option<String>,
}

/// `(entryId) => void` selection callback.
pub type UserMessageSelectFn = Rc<dyn Fn(&str)>;

/// Custom user message list with selection and scrolling.
pub struct UserMessageList {
    messages: Vec<UserMessageItem>,
    selected_index: usize,
    theme: Arc<Theme>,
    pub on_select: Option<UserMessageSelectFn>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
}

impl UserMessageList {
    pub fn new(messages: Vec<UserMessageItem>, initial_selected_id: Option<&str>, theme: Arc<Theme>) -> Self {
        let initial_index = initial_selected_id
            .and_then(|id| messages.iter().position(|message| message.id == id))
            .map(|index| index as i64)
            .unwrap_or(-1);
        let selected_index = if initial_index >= 0 {
            initial_index as usize
        } else {
            messages.len().saturating_sub(1)
        };
        Self {
            messages,
            selected_index,
            theme,
            on_select: None,
            on_cancel: None,
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }
}

impl Component for UserMessageList {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        if self.messages.is_empty() {
            lines.push(self.theme.fg(ThemeColor::Muted, "  No user messages found"));
            return lines;
        }

        let half_visible = (MAX_VISIBLE / 2) as i64;
        let max_start = self.messages.len() as i64 - MAX_VISIBLE as i64;
        let start_index = (self.selected_index as i64 - half_visible).min(max_start).max(0) as usize;
        let end_index = (start_index + MAX_VISIBLE).min(self.messages.len());

        for index in start_index..end_index {
            let message = &self.messages[index];
            let is_selected = index == self.selected_index;
            let normalized_message = js_trim(&message.text.replace('\n', " ")).to_string();
            let cursor = if is_selected {
                self.theme.fg(ThemeColor::Accent, "› ")
            } else {
                "  ".to_string()
            };
            let max_message_width = width.saturating_sub(2);
            let truncated_message = truncate_to_width(&normalized_message, max_message_width);
            let message_line = cursor
                + &if is_selected {
                    self.theme.bold(&truncated_message)
                } else {
                    truncated_message
                };
            lines.push(message_line);

            let position = index + 1;
            let metadata = format!("  Message {position} of {}", self.messages.len());
            lines.push(self.theme.fg(ThemeColor::Muted, &metadata));
            lines.push(String::new());
        }

        if start_index > 0 || end_index < self.messages.len() {
            let scroll_info = format!("  ({}/{})", self.selected_index + 1, self.messages.len());
            lines.push(self.theme.fg(ThemeColor::Muted, &scroll_info));
        }
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "tui.select.up") {
            self.selected_index = if self.selected_index == 0 {
                self.messages.len().saturating_sub(1)
            } else {
                self.selected_index - 1
            };
        } else if keybindings.matches(data, "tui.select.down") {
            self.selected_index = if self.selected_index == self.messages.len().saturating_sub(1) {
                0
            } else {
                self.selected_index + 1
            };
        } else if keybindings.matches(data, "tui.select.confirm") {
            if let (Some(selected), Some(on_select)) = (self.messages.get(self.selected_index), &self.on_select) {
                on_select(&selected.id);
            }
        } else if keybindings.matches(data, "tui.select.cancel")
            && let Some(on_cancel) = &self.on_cancel
        {
            on_cancel();
        }
    }
}

/// Renders a user message selector for branching.
pub struct UserMessageSelectorComponent {
    top: Container,
    message_list: UserMessageList,
    bottom: Container,
    auto_cancel_at: Option<Instant>,
    auto_cancel_fired: bool,
}

impl UserMessageSelectorComponent {
    pub fn new(
        messages: Vec<UserMessageItem>,
        on_select: UserMessageSelectFn,
        on_cancel: Rc<dyn Fn()>,
        initial_selected_id: Option<&str>,
        theme: Arc<Theme>,
    ) -> Self {
        let mut top = Container::new();
        top.add_child(component(Spacer::new(1)));
        top.add_child(component(Text::new(theme.bold("Fork from Message"), 1, 0)));
        top.add_child(component(Text::new(
            theme.fg(
                ThemeColor::Muted,
                "Select a user message to copy the active path up to that point into a new session",
            ),
            1,
            0,
        )));
        top.add_child(component(Spacer::new(1)));
        top.add_child(component(dynamic_border(&theme)));
        top.add_child(component(Spacer::new(1)));

        let is_empty = messages.is_empty();
        let mut message_list = UserMessageList::new(messages, initial_selected_id, theme.clone());
        message_list.on_select = Some(on_select);
        message_list.on_cancel = Some(on_cancel);

        let mut bottom = Container::new();
        bottom.add_child(component(Spacer::new(1)));
        bottom.add_child(component(dynamic_border(&theme)));

        let auto_cancel_at = if is_empty {
            Some(Instant::now() + AUTO_CANCEL_DELAY)
        } else {
            None
        };

        Self {
            top,
            message_list,
            bottom,
            auto_cancel_at,
            auto_cancel_fired: false,
        }
    }

    pub fn message_list(&self) -> &UserMessageList {
        &self.message_list
    }

    pub fn message_list_mut(&mut self) -> &mut UserMessageList {
        &mut self.message_list
    }

    /// `setTimeout(() => onCancel(), 100)` for the empty-message case.
    pub fn poll(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.auto_cancel_at else {
            return false;
        };
        if self.auto_cancel_fired || now < deadline {
            return false;
        }
        self.auto_cancel_fired = true;
        if let Some(on_cancel) = &self.message_list.on_cancel {
            on_cancel();
        }
        true
    }
}

impl Component for UserMessageSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.message_list.render(width));
        lines.extend(self.bottom.render_children(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        self.message_list.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.top.invalidate_children();
        self.message_list.invalidate();
        self.bottom.invalidate_children();
    }
}
