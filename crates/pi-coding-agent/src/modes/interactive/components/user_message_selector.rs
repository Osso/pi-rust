//! Ported from `packages/coding-agent/src/modes/interactive/components/user-message-selector.ts`.

use pi_js_compat::js_json::{JsString, encode_node_utf8};

use std::rc::Rc;
use std::time::{Duration, Instant};

use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, component};
use pi_tui::utils::truncate_to_width;

use super::super::theme::{ThemeColor, with_theme};
use super::dynamic_border::dynamic_border;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserMessageItem {
    /// Entry ID in the session.
    pub id: String,
    pub text: JsString,
}

/// Scrollable user-message list used for forking.
pub struct UserMessageList {
    messages: Vec<UserMessageItem>,
    selected_index: usize,
    max_visible: usize,
    pub on_select: Option<Rc<dyn Fn(String)>>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
}

impl UserMessageList {
    pub fn new(messages: Vec<UserMessageItem>, initial_selected_id: Option<&str>) -> Self {
        let initial_index = initial_selected_id
            .and_then(|id| messages.iter().position(|message| message.id == id))
            .map(|index| index as isize)
            .unwrap_or(-1);
        let selected_index = if initial_index >= 0 {
            initial_index as usize
        } else {
            messages.len().saturating_sub(1)
        };
        Self {
            messages,
            selected_index,
            max_visible: 10,
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
            lines.push(with_theme(|theme| {
                theme.fg(ThemeColor::Muted, "  No user messages found")
            }));
            return lines;
        }

        let length = self.messages.len();
        let half_visible = self.max_visible / 2;
        let start_index = self
            .selected_index
            .saturating_sub(half_visible)
            .min(length.saturating_sub(self.max_visible));
        let end_index = (start_index + self.max_visible).min(length);

        for index in start_index..end_index {
            let message = &self.messages[index];
            let is_selected = index == self.selected_index;
            let normalized_message = encode_node_utf8(&message.text.replace("\n", " ").trim());
            let cursor = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "› "))
            } else {
                "  ".to_owned()
            };
            let max_message_width = width.saturating_sub(2);
            let truncated_message = truncate_to_width(&normalized_message, max_message_width);
            let message_line = cursor
                + &if is_selected {
                    with_theme(|theme| theme.bold(&truncated_message))
                } else {
                    truncated_message
                };
            lines.push(message_line);

            let metadata = format!("  Message {} of {}", index + 1, length);
            lines.push(with_theme(|theme| theme.fg(ThemeColor::Muted, &metadata)));
            lines.push(String::new());
        }

        if start_index > 0 || end_index < length {
            let scroll_info = format!("  ({}/{})", self.selected_index + 1, length);
            lines.push(with_theme(|theme| theme.fg(ThemeColor::Muted, &scroll_info)));
        }

        lines
    }

    fn handle_input(&mut self, key_data: &str) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "tui.select.up") {
            self.selected_index = if self.selected_index == 0 {
                self.messages.len() - 1
            } else {
                self.selected_index - 1
            };
        } else if keybindings.matches(key_data, "tui.select.down") {
            self.selected_index = if self.selected_index == self.messages.len() - 1 {
                0
            } else {
                self.selected_index + 1
            };
        } else if keybindings.matches(key_data, "tui.select.confirm") {
            if let Some(selected) = self.messages.get(self.selected_index)
                && let Some(on_select) = &self.on_select
            {
                on_select(selected.id.clone());
            }
        } else if keybindings.matches(key_data, "tui.select.cancel")
            && let Some(on_cancel) = &self.on_cancel
        {
            on_cancel();
        }
    }

    fn invalidate(&mut self) {}
}

/// Fork-from-message selector: header, borders, and the message list.
pub struct UserMessageSelectorComponent {
    message_list: ComponentRef,
    prefix_children: Vec<ComponentRef>,
    suffix_children: Vec<ComponentRef>,
    auto_cancel_deadline: Option<Instant>,
    on_cancel: Rc<dyn Fn()>,
}

impl UserMessageSelectorComponent {
    pub fn new(
        messages: Vec<UserMessageItem>,
        on_select: Rc<dyn Fn(String)>,
        on_cancel: Rc<dyn Fn()>,
        initial_selected_id: Option<&str>,
    ) -> Self {
        let prefix_children = vec![
            component(Spacer::new(1)),
            component(Text::new(with_theme(|theme| theme.bold("Fork from Message")), 1, 0)),
            component(Text::new(
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Muted,
                        "Select a user message to copy the active path up to that point into a new session",
                    )
                }),
                1,
                0,
            )),
            component(Spacer::new(1)),
            component(dynamic_border()),
            component(Spacer::new(1)),
        ];
        let suffix_children = vec![component(Spacer::new(1)), component(dynamic_border())];

        let empty = messages.is_empty();
        let mut message_list = UserMessageList::new(messages, initial_selected_id);
        message_list.on_select = Some(on_select);
        message_list.on_cancel = Some(Rc::clone(&on_cancel));
        let message_list = component(message_list);

        Self {
            message_list,
            prefix_children,
            suffix_children,
            auto_cancel_deadline: empty.then(|| Instant::now() + Duration::from_millis(100)),
            on_cancel,
        }
    }

    pub fn get_message_list(&self) -> ComponentRef {
        Rc::clone(&self.message_list)
    }

    /// The pending empty-list auto-cancel (`setTimeout(onCancel, 100)`).
    pub fn auto_cancel_deadline(&self) -> Option<Instant> {
        self.auto_cancel_deadline
    }

    /// Fire the auto-cancel when its deadline has passed.
    pub fn poll_auto_cancel(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.auto_cancel_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.auto_cancel_deadline = None;
        (self.on_cancel)();
        true
    }
}

impl Component for UserMessageSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for child in &self.prefix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines.extend(self.message_list.borrow_mut().render(width));
        for child in &self.suffix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        for child in &self.prefix_children {
            child.borrow_mut().invalidate();
        }
        self.message_list.borrow_mut().invalidate();
        for child in &self.suffix_children {
            child.borrow_mut().invalidate();
        }
    }
}
