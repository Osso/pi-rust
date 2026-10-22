//! Ported from `packages/tui/src/components/settings-list.ts`.
//!
//! Settings list with optional fuzzy search and submenus. The TypeScript class
//! mutates items in place; Rust keeps the mutable list plus submenu state behind
//! a shared `RefCell` so submenu `done` callbacks can update the item later.

use std::cell::RefCell;
use std::rc::Rc;

use super::input::{Input, InputOptions};
use crate::fuzzy::fuzzy_filter;
use crate::keybindings::get_keybindings;
use crate::tui::{Component, ComponentRef};
use crate::utils::{truncate_to_width, truncate_to_width_with, visible_width, wrap_text_with_ansi};

pub type SubmenuDone = Rc<dyn Fn(Option<String>)>;
pub type SubmenuFactory = dyn Fn(&str, SubmenuDone) -> ComponentRef;

pub struct SettingItem {
    /// Unique identifier for this setting.
    pub id: String,
    /// Display label (left side).
    pub label: String,
    /// Optional description shown when selected.
    pub description: Option<String>,
    /// Current value to display (right side).
    pub current_value: String,
    /// If provided, Enter/Space cycles through these values.
    pub values: Option<Vec<String>>,
    /// If provided, Enter opens this submenu.
    pub submenu: Option<Rc<SubmenuFactory>>,
}

pub type SettingsLabelFn = Rc<dyn Fn(&str, bool) -> String>;
pub type SettingsTextFn = Rc<dyn Fn(&str) -> String>;
pub type SettingsChangeFn = Rc<dyn Fn(&str, &str)>;

pub struct SettingsListTheme {
    pub label: SettingsLabelFn,
    pub value: SettingsLabelFn,
    pub description: SettingsTextFn,
    pub cursor: String,
    pub hint: SettingsTextFn,
}

impl SettingsListTheme {
    pub fn plain(cursor: impl Into<String>) -> Self {
        Self {
            label: Rc::new(|text: &str, _| text.to_string()),
            value: Rc::new(|text: &str, _| text.to_string()),
            description: Rc::new(|text: &str| text.to_string()),
            cursor: cursor.into(),
            hint: Rc::new(|text: &str| text.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SettingsListOptions {
    pub enable_search: bool,
}

struct SettingsListState {
    items: Vec<SettingItem>,
    filtered_indices: Vec<usize>,
    selected_index: usize,
    submenu: Option<ComponentRef>,
    submenu_item_index: Option<usize>,
}

pub struct SettingsList {
    state: Rc<RefCell<SettingsListState>>,
    theme: SettingsListTheme,
    max_visible: usize,
    on_change: SettingsChangeFn,
    on_cancel: Rc<dyn Fn()>,
    search_input: Option<Input>,
    search_enabled: bool,
}

impl SettingsList {
    pub fn new(
        items: Vec<SettingItem>,
        max_visible: usize,
        theme: SettingsListTheme,
        on_change: SettingsChangeFn,
        on_cancel: Rc<dyn Fn()>,
        options: SettingsListOptions,
    ) -> Self {
        let filtered_indices = (0..items.len()).collect();
        Self {
            state: Rc::new(RefCell::new(SettingsListState {
                items,
                filtered_indices,
                selected_index: 0,
                submenu: None,
                submenu_item_index: None,
            })),
            theme,
            max_visible,
            on_change,
            on_cancel,
            search_input: options.enable_search.then(|| Input::new(InputOptions::default())),
            search_enabled: options.enable_search,
        }
    }

    /// Update an item's `current_value`.
    pub fn update_value(&mut self, id: &str, new_value: impl Into<String>) {
        let mut state = self.state.borrow_mut();
        if let Some(item) = state.items.iter_mut().find(|item| item.id == id) {
            item.current_value = new_value.into();
        }
    }

    pub fn selected_index(&self) -> usize {
        self.state.borrow().selected_index
    }

    pub fn is_submenu_open(&self) -> bool {
        self.state.borrow().submenu.is_some()
    }

    fn display_indices(&self, state: &SettingsListState) -> Vec<usize> {
        if self.search_enabled {
            state.filtered_indices.clone()
        } else {
            (0..state.items.len()).collect()
        }
    }

    fn render_main_list(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();

        if self.search_enabled
            && let Some(search_input) = &mut self.search_input
        {
            lines.extend(search_input.render(width));
            lines.push(String::new());
        }

        let state = self.state.borrow();
        if state.items.is_empty() {
            lines.push((self.theme.hint)("  No settings available"));
            drop(state);
            if self.search_enabled {
                self.add_hint_line(&mut lines, width);
            }
            return lines;
        }

        let display_indices = self.display_indices(&state);
        if display_indices.is_empty() {
            lines.push(truncate_to_width(&(self.theme.hint)("  No matching settings"), width));
            drop(state);
            self.add_hint_line(&mut lines, width);
            return lines;
        }

        let start_index = state
            .selected_index
            .saturating_sub(self.max_visible / 2)
            .min(display_indices.len().saturating_sub(self.max_visible));
        let end_index = (start_index + self.max_visible).min(display_indices.len());

        let max_label_width = state
            .items
            .iter()
            .map(|item| visible_width(&item.label))
            .max()
            .unwrap_or(0)
            .min(30);

        for (index, item_index) in display_indices.iter().enumerate().take(end_index).skip(start_index) {
            let item = &state.items[*item_index];
            let is_selected = index == state.selected_index;
            let prefix = if is_selected {
                self.theme.cursor.clone()
            } else {
                "  ".to_string()
            };
            let prefix_width = visible_width(&prefix);
            let label_padding = " ".repeat(max_label_width.saturating_sub(visible_width(&item.label)));
            let label_padded = format!("{}{label_padding}", item.label);
            let label_text = (self.theme.label)(&label_padded, is_selected);

            const SEPARATOR: &str = "  ";
            let used_width = prefix_width + max_label_width + visible_width(SEPARATOR);
            let value_max_width = width as i64 - used_width as i64 - 2;
            let truncated_value = if value_max_width <= 0 {
                String::new()
            } else {
                truncate_to_width_with(&item.current_value, value_max_width as usize, "", false)
            };
            let value_text = (self.theme.value)(&truncated_value, is_selected);
            lines.push(truncate_to_width(
                &format!("{prefix}{label_text}{SEPARATOR}{value_text}"),
                width,
            ));
        }

        if start_index > 0 || end_index < display_indices.len() {
            let scroll_text = format!("  ({}/{})", state.selected_index + 1, display_indices.len());
            lines.push((self.theme.hint)(&truncate_to_width_with(
                &scroll_text,
                width.saturating_sub(2),
                "",
                false,
            )));
        }

        if let Some(selected_item) = display_indices
            .get(state.selected_index)
            .map(|index| &state.items[*index])
            && let Some(description) = selected_item.description.as_ref().filter(|text| !text.is_empty())
        {
            lines.push(String::new());
            for line in wrap_text_with_ansi(description, width.saturating_sub(4)) {
                lines.push((self.theme.description)(&format!("  {line}")));
            }
        }
        drop(state);

        self.add_hint_line(&mut lines, width);
        lines
    }

    fn handle_key_input(&mut self, data: &str) {
        let submenu = self.state.borrow().submenu.clone();
        if let Some(submenu) = submenu {
            submenu.borrow_mut().handle_input(data);
            return;
        }

        let keybindings = get_keybindings();
        let display_indices = self.display_indices(&self.state.borrow());
        if keybindings.matches(data, "tui.select.up") {
            if display_indices.is_empty() {
                return;
            }
            let mut state = self.state.borrow_mut();
            state.selected_index = if state.selected_index == 0 {
                display_indices.len() - 1
            } else {
                state.selected_index - 1
            };
        } else if keybindings.matches(data, "tui.select.down") {
            if display_indices.is_empty() {
                return;
            }
            let mut state = self.state.borrow_mut();
            state.selected_index = if state.selected_index == display_indices.len() - 1 {
                0
            } else {
                state.selected_index + 1
            };
        } else if keybindings.matches(data, "tui.select.confirm") || data == " " {
            self.activate_item();
        } else if keybindings.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        } else if self.search_enabled
            && let Some(search_input) = &mut self.search_input
        {
            let sanitized = data.replace(' ', "");
            if sanitized.is_empty() {
                return;
            }
            search_input.handle_input(&sanitized);
            let query = search_input.get_value().to_string();
            self.apply_filter(&query);
        }
    }

    fn activate_item(&mut self) {
        let selected_index = self.state.borrow().selected_index;
        let display_indices = self.display_indices(&self.state.borrow());
        let Some(item_index) = display_indices.get(selected_index).copied() else {
            return;
        };

        let (id, current_value, values, submenu) = {
            let state = self.state.borrow();
            let item = &state.items[item_index];
            (
                item.id.clone(),
                item.current_value.clone(),
                item.values.clone(),
                item.submenu.clone(),
            )
        };

        if let Some(submenu) = submenu {
            let state = Rc::clone(&self.state);
            let on_change = Rc::clone(&self.on_change);
            let done_id = id.clone();
            let done: SubmenuDone = Rc::new(move |selected_value| {
                if let Some(value) = selected_value {
                    {
                        let mut state = state.borrow_mut();
                        if let Some(item) = state.items.iter_mut().find(|item| item.id == done_id) {
                            item.current_value = value.clone();
                        }
                    }
                    on_change(&done_id, &value);
                }
                close_submenu(&state);
            });
            self.state.borrow_mut().submenu_item_index = Some(selected_index);
            let component = submenu(&current_value, done);
            self.state.borrow_mut().submenu = Some(component);
        } else if let Some(values) = values
            && !values.is_empty()
        {
            let current_index = values
                .iter()
                .position(|value| *value == current_value)
                .map_or(-1, |index| index as i64);
            let next_index = ((current_index + 1) % values.len() as i64) as usize;
            let new_value = values[next_index].clone();
            {
                let mut state = self.state.borrow_mut();
                if let Some(item) = state.items.iter_mut().find(|item| item.id == id) {
                    item.current_value = new_value.clone();
                }
            }
            (self.on_change)(&id, &new_value);
        }
    }

    fn apply_filter(&mut self, query: &str) {
        let filtered_indices = {
            let state = self.state.borrow();
            let items = &state.items;
            let indices: Vec<usize> = (0..items.len()).collect();
            fuzzy_filter(&indices, query, |index| vec![items[*index].label.clone()])
        };
        let mut state = self.state.borrow_mut();
        state.filtered_indices = filtered_indices;
        state.selected_index = 0;
    }

    fn add_hint_line(&self, lines: &mut Vec<String>, width: usize) {
        lines.push(String::new());
        let text = if self.search_enabled {
            "  Type to search · Enter/Space to change · Esc to cancel"
        } else {
            "  Enter/Space to change · Esc to cancel"
        };
        lines.push(truncate_to_width(&(self.theme.hint)(text), width));
    }
}

fn close_submenu(state: &Rc<RefCell<SettingsListState>>) {
    let mut state = state.borrow_mut();
    state.submenu = None;
    if let Some(index) = state.submenu_item_index.take() {
        state.selected_index = index;
    }
}

impl Component for SettingsList {
    fn render(&mut self, width: usize) -> Vec<String> {
        let submenu = self.state.borrow().submenu.clone();
        if let Some(submenu) = submenu {
            return submenu.borrow_mut().render(width);
        }
        self.render_main_list(width)
    }

    fn handle_input(&mut self, data: &str) {
        self.handle_key_input(data);
    }

    fn invalidate(&mut self) {
        let submenu = self.state.borrow().submenu.clone();
        if let Some(submenu) = submenu {
            submenu.borrow_mut().invalidate();
        }
    }
}
