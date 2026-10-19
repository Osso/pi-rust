//! Ported from `packages/coding-agent/src/modes/interactive/components/theme-selector.ts`.

use std::rc::Rc;

use pi_tui::components::select_list::{SelectItem, SelectList, SelectListLayoutOptions};
use pi_tui::tui::{Component, ComponentRef, Container, component};

use super::super::theme::{get_available_themes, get_select_list_theme};
use super::dynamic_border::dynamic_border;

const THEME_SELECT_LIST_LAYOUT: SelectListLayoutOptions = SelectListLayoutOptions {
    min_primary_column_width: Some(12),
    max_primary_column_width: Some(32),
    truncate_primary: None,
};

/// Theme selector with a live preview on selection change.
pub struct ThemeSelectorComponent {
    container: Container,
    select_list: ComponentRef,
}

impl ThemeSelectorComponent {
    pub fn new(
        current_theme: &str,
        on_select: Rc<dyn Fn(String)>,
        on_cancel: Rc<dyn Fn()>,
        on_preview: Rc<dyn Fn(String)>,
    ) -> Self {
        let themes = get_available_themes().unwrap_or_else(|error| {
            crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.to_string())
        });
        let theme_items: Vec<SelectItem> = themes
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

        let mut container = Container::new();
        container.add_child(component(dynamic_border()));

        let mut select_list = SelectList::new(theme_items, 10, get_select_list_theme(), THEME_SELECT_LIST_LAYOUT);
        if let Some(index) = themes.iter().position(|name| name == current_theme) {
            select_list.set_selected_index(index as isize);
        }
        select_list.on_select = Some(Rc::new(move |item: &SelectItem| on_select(item.value.clone())));
        select_list.on_cancel = Some(on_cancel);
        select_list.on_selection_change = Some(Rc::new(move |item: &SelectItem| on_preview(item.value.clone())));

        let select_list = component(select_list);
        container.add_child(Rc::clone(&select_list));
        container.add_child(component(dynamic_border()));

        Self { container, select_list }
    }

    pub fn get_select_list(&self) -> ComponentRef {
        Rc::clone(&self.select_list)
    }
}

impl Component for ThemeSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.container.render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}
