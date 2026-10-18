//! Ported from `coding-agent/src/modes/interactive/components/show-images-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_tui::{Component, DynamicBorder, SelectItem, SelectList, SelectListLayoutOptions};

use crate::modes::interactive::components::dynamic_border::dynamic_border;

use crate::modes::interactive::theme::{Theme, get_select_list_theme};

const SHOW_IMAGES_SELECT_LIST_LAYOUT: SelectListLayoutOptions = SelectListLayoutOptions {
    min_primary_column_width: Some(12),
    max_primary_column_width: Some(32),
    truncate_primary: None,
};

/// Show-images selector with top and bottom borders.
pub struct ShowImagesSelectorComponent {
    top_border: DynamicBorder,
    select_list: SelectList,
    bottom_border: DynamicBorder,
}

impl ShowImagesSelectorComponent {
    pub fn new(current_value: bool, on_select: Rc<dyn Fn(bool)>, on_cancel: Rc<dyn Fn()>, theme: Arc<Theme>) -> Self {
        let items = vec![
            SelectItem::new("yes", "Yes").with_description("Show images inline in terminal"),
            SelectItem::new("no", "No").with_description("Show text placeholder instead"),
        ];

        let mut select_list = SelectList::new(items, 5, get_select_list_theme(), SHOW_IMAGES_SELECT_LIST_LAYOUT);
        select_list.set_selected_index(if current_value { 0 } else { 1 });
        select_list.on_select = Some(Rc::new(move |item: &SelectItem| on_select(item.value == "yes")));
        select_list.on_cancel = Some(on_cancel);

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

impl Component for ShowImagesSelectorComponent {
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
