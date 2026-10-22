//! Ported from `packages/coding-agent/src/modes/interactive/components/show-images-selector.ts`.

use std::rc::Rc;

use pi_js_compat::js_json::JsString;

use pi_tui::components::select_list::{SelectItem, SelectList, SelectListLayoutOptions};
use pi_tui::tui::{Component, ComponentRef, Container, component};

use super::super::theme::get_select_list_theme;
use super::dynamic_border::dynamic_border;

const SHOW_IMAGES_SELECT_LIST_LAYOUT: SelectListLayoutOptions = SelectListLayoutOptions {
    min_primary_column_width: Some(12),
    max_primary_column_width: Some(32),
    truncate_primary: None,
};

/// Inline-image toggle selector with borders.
pub struct ShowImagesSelectorComponent {
    container: Container,
    select_list: ComponentRef,
}

impl ShowImagesSelectorComponent {
    pub fn new(current_value: bool, on_select: Rc<dyn Fn(bool)>, on_cancel: Rc<dyn Fn()>) -> Self {
        let items = vec![
            SelectItem::new("yes", "Yes").with_description("Show images inline in terminal"),
            SelectItem::new("no", "No").with_description("Show text placeholder instead"),
        ];

        let mut container = Container::new();
        container.add_child(component(dynamic_border()));

        let mut select_list = SelectList::new(items, 5, get_select_list_theme(), SHOW_IMAGES_SELECT_LIST_LAYOUT);
        select_list.set_selected_index(if current_value { 0 } else { 1 });
        select_list.on_select = Some(Rc::new(move |item: &SelectItem| on_select(item.value == "yes")));
        select_list.on_cancel = Some(on_cancel);

        let select_list = component(select_list);
        container.add_child(Rc::clone(&select_list));
        container.add_child(component(dynamic_border()));

        Self { container, select_list }
    }

    pub fn get_select_list(&self) -> ComponentRef {
        Rc::clone(&self.select_list)
    }
}

impl Component for ShowImagesSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}
