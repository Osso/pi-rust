//! Ported from `packages/coding-agent/src/modes/interactive/components/thinking-selector.ts`.

use std::rc::Rc;

use pi_agent_core::ThinkingLevel;
use pi_js_compat::js_json::JsString;
use pi_tui::components::select_list::{SelectItem, SelectList, SelectListLayoutOptions};
use pi_tui::tui::{Component, ComponentRef, Container, component};

use super::super::theme::get_select_list_theme;
use super::dynamic_border::dynamic_border;

const THINKING_SELECT_LIST_LAYOUT: SelectListLayoutOptions = SelectListLayoutOptions {
    min_primary_column_width: Some(12),
    max_primary_column_width: Some(32),
    truncate_primary: None,
};

fn level_description(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Off => "No reasoning",
        ThinkingLevel::Minimal => "Very brief reasoning (~1k tokens)",
        ThinkingLevel::Low => "Light reasoning (~2k tokens)",
        ThinkingLevel::Medium => "Moderate reasoning (~8k tokens)",
        ThinkingLevel::High => "Deep reasoning (~16k tokens)",
        ThinkingLevel::XHigh => "Extra-high reasoning (~32k tokens)",
        ThinkingLevel::Max => "Maximum reasoning depth",
        ThinkingLevel::Ultra => "Maximum reasoning with automatic delegation",
    }
}

fn parse_level(value: &JsString) -> Option<ThinkingLevel> {
    [
        ThinkingLevel::Off,
        ThinkingLevel::Minimal,
        ThinkingLevel::Low,
        ThinkingLevel::Medium,
        ThinkingLevel::High,
        ThinkingLevel::XHigh,
        ThinkingLevel::Max,
        ThinkingLevel::Ultra,
    ]
    .into_iter()
    .find(|level| value == level.as_str())
}

/// Thinking level selector with borders.
pub struct ThinkingSelectorComponent {
    container: Container,
    select_list: ComponentRef,
}

impl ThinkingSelectorComponent {
    pub fn new(
        current_level: ThinkingLevel,
        available_levels: &[ThinkingLevel],
        on_select: Rc<dyn Fn(ThinkingLevel)>,
        on_cancel: Rc<dyn Fn()>,
    ) -> Self {
        let thinking_levels: Vec<SelectItem> = available_levels
            .iter()
            .map(|level| SelectItem::new(level.as_str(), level.as_str()).with_description(level_description(*level)))
            .collect();

        let mut container = Container::new();
        container.add_child(component(dynamic_border()));

        let mut select_list = SelectList::new(
            thinking_levels.clone(),
            thinking_levels.len(),
            get_select_list_theme(),
            THINKING_SELECT_LIST_LAYOUT,
        );
        if let Some(index) = thinking_levels
            .iter()
            .position(|item| item.value == current_level.as_str())
        {
            select_list.set_selected_index(index as isize);
        }
        select_list.on_select = Some(Rc::new(move |item: &SelectItem| {
            if let Some(level) = parse_level(&item.value) {
                on_select(level);
            }
        }));
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

impl Component for ThinkingSelectorComponent {
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
