//! Ported from `coding-agent/src/modes/interactive/components/theme-selector.ts`.

use std::rc::Rc;

use pi_tui::{Component, DynamicBorder, SelectItem, SelectList, SelectListLayoutOptions};

use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::theme_api::{ThemeApi, select_list_theme};

const THEME_SELECT_LIST_LAYOUT: SelectListLayoutOptions = SelectListLayoutOptions {
    min_primary_column_width: Some(12),
    max_primary_column_width: Some(32),
    truncate_primary: None,
};

/// Theme selector with preview on selection change.
pub struct ThemeSelectorComponent {
    top_border: DynamicBorder,
    select_list: SelectList,
    bottom_border: DynamicBorder,
}

impl ThemeSelectorComponent {
    /// `getAvailableThemes()` is supplied by the caller because the theme
    /// registry lives in the interactive theme slice.
    pub fn new(
        themes: &[String],
        current_theme: &str,
        on_select: Rc<dyn Fn(String)>,
        on_cancel: Rc<dyn Fn()>,
        on_preview: Rc<dyn Fn(String)>,
        theme: Rc<dyn ThemeApi>,
    ) -> Self {
        let items: Vec<SelectItem> = themes
            .iter()
            .map(|name| {
                let item = SelectItem::new(name.clone(), name.clone());
                if name == current_theme {
                    item.with_description("(current)")
                } else {
                    item
                }
            })
            .collect();

        let mut select_list = SelectList::new(items, 10, select_list_theme(&theme), THEME_SELECT_LIST_LAYOUT);
        if let Some(current_index) = themes.iter().position(|name| name == current_theme) {
            select_list.set_selected_index(current_index as isize);
        }
        select_list.on_select = Some(Rc::new(move |item: &SelectItem| on_select(item.value.clone())));
        select_list.on_cancel = Some(on_cancel);
        select_list.on_selection_change = Some(Rc::new(move |item: &SelectItem| on_preview(item.value.clone())));

        Self {
            top_border: dynamic_border(&theme),
            select_list,
            bottom_border: dynamic_border(&theme),
        }
    }

    pub fn select_list(&self) -> &SelectList {
        &self.select_list
    }

    pub fn select_list_mut(&mut self) -> &mut SelectList {
        &mut self.select_list
    }
}

impl Component for ThemeSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top_border.render(width);
        lines.extend(self.select_list.render(width));
        lines.extend(self.bottom_border.render(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        self.select_list.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.select_list.invalidate();
    }
}
