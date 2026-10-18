//! Ported from `packages/tui/src/components/select-list.ts`.
//!
//! A scrolling single-column selection list with an aligned description column
//! and configurable primary-column truncation.

use std::rc::Rc;

use super::text::StyleFn;
use crate::keybindings::get_keybindings;
use crate::tui::Component;
use crate::utils::{truncate_to_width_with, visible_width};

const DEFAULT_PRIMARY_COLUMN_WIDTH: usize = 32;
const PRIMARY_COLUMN_GAP: usize = 2;
const MIN_DESCRIPTION_WIDTH: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl SelectItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Styling callbacks for a [`SelectList`].
#[derive(Clone)]
pub struct SelectListTheme {
    pub selected_prefix: StyleFn,
    pub selected_text: StyleFn,
    pub description: StyleFn,
    pub scroll_info: StyleFn,
    pub no_match: StyleFn,
}

impl SelectListTheme {
    /// Identity theme used by tests and unstyled callers.
    pub fn plain() -> Self {
        Self {
            selected_prefix: identity(),
            selected_text: identity(),
            description: identity(),
            scroll_info: identity(),
            no_match: identity(),
        }
    }
}

fn identity() -> StyleFn {
    Rc::new(|text: &str| text.to_string())
}

/// Context passed to a custom primary-column truncator.
pub struct SelectListTruncatePrimaryContext<'a> {
    pub text: &'a str,
    pub max_width: usize,
    pub column_width: usize,
    pub item: &'a SelectItem,
    pub is_selected: bool,
}

pub type TruncatePrimaryFn = Rc<dyn Fn(&SelectListTruncatePrimaryContext<'_>) -> String>;

#[derive(Clone, Default)]
pub struct SelectListLayoutOptions {
    pub min_primary_column_width: Option<usize>,
    pub max_primary_column_width: Option<usize>,
    pub truncate_primary: Option<TruncatePrimaryFn>,
}

pub type SelectItemCallback = Rc<dyn Fn(&SelectItem)>;

pub struct SelectList {
    items: Vec<SelectItem>,
    filtered_items: Vec<SelectItem>,
    selected_index: usize,
    max_visible: usize,
    theme: SelectListTheme,
    layout: SelectListLayoutOptions,
    pub on_select: Option<SelectItemCallback>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
    pub on_selection_change: Option<SelectItemCallback>,
}

impl SelectList {
    pub fn new(
        items: Vec<SelectItem>,
        max_visible: usize,
        theme: SelectListTheme,
        layout: SelectListLayoutOptions,
    ) -> Self {
        Self {
            filtered_items: items.clone(),
            items,
            selected_index: 0,
            max_visible,
            theme,
            layout,
            on_select: None,
            on_cancel: None,
            on_selection_change: None,
        }
    }

    pub fn set_filter(&mut self, filter: &str) {
        let filter = filter.to_lowercase();
        self.filtered_items = self
            .items
            .iter()
            .filter(|item| item.value.to_lowercase().starts_with(&filter))
            .cloned()
            .collect();
        self.selected_index = 0;
    }

    pub fn set_selected_index(&mut self, index: isize) {
        let max_index = self.filtered_items.len() as isize - 1;
        self.selected_index = index.clamp(0, max_index.max(0)) as usize;
    }

    pub fn get_selected_item(&self) -> Option<&SelectItem> {
        self.filtered_items.get(self.selected_index)
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn get_display_value(item: &SelectItem) -> &str {
        if item.label.is_empty() {
            &item.value
        } else {
            &item.label
        }
    }

    fn get_primary_column_bounds(&self) -> (usize, usize) {
        let raw_min = self
            .layout
            .min_primary_column_width
            .or(self.layout.max_primary_column_width)
            .unwrap_or(DEFAULT_PRIMARY_COLUMN_WIDTH);
        let raw_max = self
            .layout
            .max_primary_column_width
            .or(self.layout.min_primary_column_width)
            .unwrap_or(DEFAULT_PRIMARY_COLUMN_WIDTH);
        (raw_min.min(raw_max).max(1), raw_max.max(raw_min).max(1))
    }

    fn get_primary_column_width(&self) -> usize {
        let (min, max) = self.get_primary_column_bounds();
        let widest = self
            .filtered_items
            .iter()
            .map(|item| visible_width(Self::get_display_value(item)) + PRIMARY_COLUMN_GAP)
            .max()
            .unwrap_or(0);
        widest.clamp(min, max)
    }

    fn truncate_primary(&self, item: &SelectItem, is_selected: bool, max_width: usize, column_width: usize) -> String {
        let display_value = Self::get_display_value(item);
        let truncated = match &self.layout.truncate_primary {
            Some(truncate_primary) => truncate_primary(&SelectListTruncatePrimaryContext {
                text: display_value,
                max_width,
                column_width,
                item,
                is_selected,
            }),
            None => truncate_to_width_with(display_value, max_width, "", false),
        };
        truncate_to_width_with(&truncated, max_width, "", false)
    }

    fn render_item(
        &self,
        item: &SelectItem,
        is_selected: bool,
        width: usize,
        description_single_line: Option<&str>,
        primary_column_width: usize,
    ) -> String {
        let prefix = if is_selected { "→ " } else { "  " };
        let prefix_width = visible_width(prefix);

        if let Some(description) = description_single_line
            && width > 40
        {
            let effective_primary_column_width =
                primary_column_width.min(width.saturating_sub(prefix_width + 4)).max(1);
            let max_primary_width = effective_primary_column_width.saturating_sub(PRIMARY_COLUMN_GAP).max(1);
            let truncated_value =
                self.truncate_primary(item, is_selected, max_primary_width, effective_primary_column_width);
            let truncated_value_width = visible_width(&truncated_value);
            let spacing = " ".repeat(
                effective_primary_column_width
                    .saturating_sub(truncated_value_width)
                    .max(1),
            );
            let description_start = prefix_width + truncated_value_width + spacing.len();
            let remaining_width = width.saturating_sub(description_start + 2);

            if remaining_width > MIN_DESCRIPTION_WIDTH {
                let truncated_desc = truncate_to_width_with(description, remaining_width, "", false);
                if is_selected {
                    return (self.theme.selected_text)(&format!("{prefix}{truncated_value}{spacing}{truncated_desc}"));
                }

                let desc_text = (self.theme.description)(&format!("{spacing}{truncated_desc}"));
                return format!("{prefix}{truncated_value}{desc_text}");
            }
        }

        let max_width = width.saturating_sub(prefix_width + 2);
        let truncated_value = self.truncate_primary(item, is_selected, max_width, max_width);
        if is_selected {
            return (self.theme.selected_text)(&format!("{prefix}{truncated_value}"));
        }

        format!("{prefix}{truncated_value}")
    }

    fn notify_selection_change(&self) {
        if let (Some(item), Some(callback)) = (self.get_selected_item(), &self.on_selection_change) {
            callback(item);
        }
    }
}

impl Component for SelectList {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();

        if self.filtered_items.is_empty() {
            lines.push((self.theme.no_match)("  No matching commands"));
            return lines;
        }

        let primary_column_width = self.get_primary_column_width();
        let start_index = self
            .selected_index
            .saturating_sub(self.max_visible / 2)
            .min(self.filtered_items.len().saturating_sub(self.max_visible));
        let end_index = (start_index + self.max_visible).min(self.filtered_items.len());

        for index in start_index..end_index {
            let item = &self.filtered_items[index];
            let is_selected = index == self.selected_index;
            let description_single_line = item.description.as_deref().map(normalize_to_single_line);
            lines.push(self.render_item(
                item,
                is_selected,
                width,
                description_single_line.as_deref(),
                primary_column_width,
            ));
        }

        if start_index > 0 || end_index < self.filtered_items.len() {
            let scroll_text = format!("  ({}/{})", self.selected_index + 1, self.filtered_items.len());
            lines.push((self.theme.scroll_info)(&truncate_to_width_with(
                &scroll_text,
                width.saturating_sub(2),
                "",
                false,
            )));
        }

        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.up") {
            if !self.filtered_items.is_empty() {
                self.selected_index = if self.selected_index == 0 {
                    self.filtered_items.len() - 1
                } else {
                    self.selected_index - 1
                };
            }
            self.notify_selection_change();
        } else if keybindings.matches(data, "tui.select.down") {
            if !self.filtered_items.is_empty() {
                self.selected_index = if self.selected_index == self.filtered_items.len() - 1 {
                    0
                } else {
                    self.selected_index + 1
                };
            }
            self.notify_selection_change();
        } else if keybindings.matches(data, "tui.select.confirm") {
            if let (Some(item), Some(callback)) = (self.get_selected_item(), &self.on_select) {
                callback(item);
            }
        } else if keybindings.matches(data, "tui.select.cancel")
            && let Some(callback) = &self.on_cancel
        {
            callback();
        }
    }
}

fn normalize_to_single_line(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_break = false;
    for character in text.chars() {
        if character == '\r' || character == '\n' {
            if !in_break {
                result.push(' ');
            }
            in_break = true;
        } else {
            in_break = false;
            result.push(character);
        }
    }
    result.trim().to_string()
}
