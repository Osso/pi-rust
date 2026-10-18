//! Ported from `coding-agent/src/modes/interactive/components/thinking-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_tui::{Component, DynamicBorder, SelectItem, SelectList, SelectListLayoutOptions};

use crate::modes::interactive::components::dynamic_border::dynamic_border;

use crate::modes::interactive::theme::{Theme, get_select_list_theme};

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

/// Thinking level selector with top and bottom borders.
pub struct ThinkingSelectorComponent {
    top_border: DynamicBorder,
    select_list: SelectList,
    bottom_border: DynamicBorder,
}

impl ThinkingSelectorComponent {
    pub fn new(
        current_level: ThinkingLevel,
        available_levels: &[ThinkingLevel],
        on_select: Rc<dyn Fn(ThinkingLevel)>,
        on_cancel: Rc<dyn Fn()>,
        theme: Arc<Theme>,
    ) -> Self {
        let levels: Vec<ThinkingLevel> = available_levels.to_vec();
        let items: Vec<SelectItem> = levels
            .iter()
            .map(|level| SelectItem::new(level.as_str(), level.as_str()).with_description(level_description(*level)))
            .collect();
        let max_visible = items.len();

        let mut select_list = SelectList::new(items, max_visible, get_select_list_theme(), THINKING_SELECT_LIST_LAYOUT);
        if let Some(current_index) = levels.iter().position(|level| *level == current_level) {
            select_list.set_selected_index(current_index as isize);
        }
        select_list.on_select = Some(Rc::new(move |item: &SelectItem| {
            if let Some(level) = levels.iter().find(|level| level.as_str() == item.value) {
                on_select(*level);
            }
        }));
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

impl Component for ThinkingSelectorComponent {
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
