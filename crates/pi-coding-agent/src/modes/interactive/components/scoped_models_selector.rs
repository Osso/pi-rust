//! Ported from `coding-agent/src/modes/interactive/components/scoped-models-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_ai::types::Model;
use pi_tui::components::input::InputOptions;
use pi_tui::tui::component;
use pi_tui::{Component, Container, Focusable, Input, Spacer, Text, fuzzy_filter, matches_key};
use std::collections::HashMap;

use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::key_text;
use crate::modes::interactive::model_search::{ModelSearchItem, get_model_search_candidates};
use crate::modes::interactive::theme::{Theme, ThemeColor};

/// `EnabledIds`: `None` means all enabled (no filter).
type EnabledIds = Option<Vec<String>>;

fn is_enabled(enabled_ids: &EnabledIds, id: &str) -> bool {
    enabled_ids
        .as_ref()
        .is_none_or(|enabled| enabled.iter().any(|enabled_id| enabled_id == id))
}

fn toggle(enabled_ids: EnabledIds, id: &str) -> EnabledIds {
    let Some(mut list) = enabled_ids else {
        return Some(vec![id.to_string()]);
    };
    if let Some(index) = list.iter().position(|enabled_id| enabled_id == id) {
        list.remove(index);
        return Some(list);
    }
    list.push(id.to_string());
    Some(list)
}

fn enable_all(enabled_ids: EnabledIds, all_ids: &[String], target_ids: Option<&[String]>) -> EnabledIds {
    let Some(mut result) = enabled_ids else {
        return None;
    };
    let targets = target_ids.unwrap_or(all_ids);
    for id in targets {
        if !result.contains(id) {
            result.push(id.clone());
        }
    }
    if result.len() == all_ids.len() && result.iter().all(|id| all_ids.contains(id)) {
        None
    } else {
        Some(result)
    }
}

fn clear_all(enabled_ids: EnabledIds, all_ids: &[String], target_ids: Option<&[String]>) -> EnabledIds {
    let Some(enabled) = enabled_ids else {
        return match target_ids {
            Some(targets) => Some(all_ids.iter().filter(|id| !targets.contains(id)).cloned().collect()),
            None => Some(Vec::new()),
        };
    };
    match target_ids {
        Some(targets) => Some(enabled.into_iter().filter(|id| !targets.contains(id)).collect()),
        None => Some(Vec::new()),
    }
}

fn move_enabled(enabled_ids: EnabledIds, id: &str, delta: i64) -> EnabledIds {
    let Some(mut list) = enabled_ids else {
        return None;
    };
    let Some(index) = list.iter().position(|enabled_id| enabled_id == id) else {
        return Some(list);
    };
    let new_index = index as i64 + delta;
    if new_index < 0 || new_index >= list.len() as i64 {
        return Some(list);
    }
    list.swap(index, new_index as usize);
    Some(list)
}

fn get_sorted_ids(enabled_ids: &EnabledIds, all_ids: &[String]) -> Vec<String> {
    match enabled_ids {
        None => all_ids.to_vec(),
        Some(enabled) => {
            let mut ids = enabled.clone();
            ids.extend(all_ids.iter().filter(|id| !enabled.contains(id)).cloned());
            ids
        }
    }
}

#[derive(Clone)]
struct ModelItem {
    full_id: String,
    model: Option<Model>,
    enabled: bool,
}

/// `ModelsConfig`.
pub struct ModelsConfig {
    pub all_models: Vec<Model>,
    pub enabled_model_ids: Option<Vec<String>>,
    pub refresh_status: Option<String>,
}

/// `ModelsCallbacks` (async callbacks become sync; callers spawn).
pub struct ModelsCallbacks {
    pub on_change: Rc<dyn Fn(Option<&[String]>)>,
    pub on_persist: Rc<dyn Fn(Option<&[String]>)>,
    pub on_cancel: Rc<dyn Fn()>,
}

/// Refresh-status color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshStatusKind {
    Muted,
    Success,
    Warning,
}

/// Model enable/order configuration selector.
pub struct ScopedModelsSelectorComponent {
    models_by_id: HashMap<String, Model>,
    all_ids: Vec<String>,
    enabled_ids: EnabledIds,
    filtered_items: Vec<ModelItem>,
    selected_index: usize,
    search_input: Input,
    list_container: Container,
    footer_text: Text,
    refresh_status_text: Option<Text>,
    callbacks: ModelsCallbacks,
    max_visible: usize,
    is_dirty: bool,
    focused: bool,
    top: Container,
    theme: Arc<Theme>,
}

impl ScopedModelsSelectorComponent {
    pub fn new(config: ModelsConfig, callbacks: ModelsCallbacks, theme: Arc<Theme>) -> Self {
        let mut models_by_id = HashMap::new();
        let mut all_ids = Vec::new();
        for model in config.all_models {
            let full_id = format!("{}/{}", model.provider, model.id);
            models_by_id.insert(full_id.clone(), model);
            all_ids.push(full_id);
        }

        let enabled_ids = config.enabled_model_ids;
        let refresh_status_text = config
            .refresh_status
            .map(|status| Text::new(theme.fg(ThemeColor::Muted, &format!("  {status}")), 0, 0));

        let mut top = Container::new();
        top.add_child(component(dynamic_border(&theme)));
        top.add_child(component(Spacer::new(1)));
        top.add_child(component(Text::new(
            theme.fg(ThemeColor::Accent, &theme.bold("Model Configuration")),
            0,
            0,
        )));
        top.add_child(component(Text::new(
            theme.fg(
                ThemeColor::Muted,
                &format!("Session-only. {} to save to settings.", key_text("app.models.save")),
            ),
            0,
            0,
        )));
        top.add_child(component(Spacer::new(1)));

        let footer_text = Text::new("", 0, 0);

        let mut selector = Self {
            models_by_id,
            all_ids,
            enabled_ids,
            filtered_items: Vec::new(),
            selected_index: 0,
            search_input: Input::new(InputOptions::default()),
            list_container: Container::new(),
            footer_text,
            refresh_status_text,
            callbacks,
            max_visible: 8,
            is_dirty: false,
            focused: false,
            top,
            theme,
        };
        selector.filtered_items = selector.build_items();
        selector.update_list();
        let footer = selector.get_footer_text();
        selector.footer_text.set_text(footer);
        selector
    }

    /// `updateModels`; `enabled_model_ids`: `None` = unchanged, `Some(None)` = all, `Some(Some(..))` = list.
    pub fn update_models(&mut self, models: &[Model], enabled_model_ids: Option<Option<Vec<String>>>) {
        let selected_id = self
            .filtered_items
            .get(self.selected_index)
            .map(|item| item.full_id.clone());
        if let Some(enabled) = enabled_model_ids {
            self.enabled_ids = enabled;
        }
        self.models_by_id.clear();
        self.all_ids.clear();
        for model in models {
            let full_id = format!("{}/{}", model.provider, model.id);
            self.models_by_id.insert(full_id.clone(), model.clone());
            self.all_ids.push(full_id);
        }
        self.refresh();
        let refreshed_index = selected_id
            .and_then(|id| self.filtered_items.iter().position(|item| item.full_id == id))
            .map_or(-1, |index| index as i64);
        if refreshed_index >= 0 {
            self.selected_index = refreshed_index as usize;
            self.update_list();
        }
    }

    pub fn set_refresh_status(&mut self, message: &str, kind: RefreshStatusKind) {
        if let Some(text) = &mut self.refresh_status_text {
            let color = match kind {
                RefreshStatusKind::Muted => ThemeColor::Muted,
                RefreshStatusKind::Success => ThemeColor::Success,
                RefreshStatusKind::Warning => ThemeColor::Warning,
            };
            text.set_text(self.theme.fg(color, &format!("  {message}")));
        }
    }

    fn build_items(&self) -> Vec<ModelItem> {
        get_sorted_ids(&self.enabled_ids, &self.all_ids)
            .into_iter()
            .map(|id| ModelItem {
                enabled: is_enabled(&self.enabled_ids, &id),
                model: self.models_by_id.get(&id).cloned(),
                full_id: id,
            })
            .collect()
    }

    fn get_footer_text(&self) -> String {
        let enabled_count = self.enabled_ids.as_ref().map_or(self.all_ids.len(), |enabled| {
            enabled.iter().filter(|id| self.models_by_id.contains_key(*id)).count()
        });
        let unavailable_count = self.enabled_ids.as_ref().map_or(0, |enabled| {
            enabled.iter().filter(|id| !self.models_by_id.contains_key(*id)).count()
        });
        let all_enabled = self.enabled_ids.is_none();
        let count_text = if all_enabled {
            "all enabled".to_string()
        } else {
            let mut text = format!("{enabled_count}/{} enabled", self.all_ids.len());
            if unavailable_count > 0 {
                text.push_str(&format!(" · {unavailable_count} unavailable"));
            }
            text
        };
        let parts = [
            format!("{} toggle", key_text("tui.select.confirm")),
            format!("{} all", key_text("app.models.enableAll")),
            format!("{} clear", key_text("app.models.clearAll")),
            format!("{} provider", key_text("app.models.toggleProvider")),
            format!(
                "{}/{} reorder",
                key_text("app.models.reorderUp"),
                key_text("app.models.reorderDown")
            ),
            format!("{} save", key_text("app.models.save")),
            count_text,
        ];
        let joined = format!("  {}", parts.join(" · "));
        if self.is_dirty {
            self.theme.fg(ThemeColor::Dim, &format!("{joined} ")) + &self.theme.fg(ThemeColor::Warning, "(unsaved)")
        } else {
            self.theme.fg(ThemeColor::Dim, &joined)
        }
    }

    fn refresh(&mut self) {
        let query = self.search_input.get_value().to_string();
        let items = self.build_items();
        self.filtered_items = if query.is_empty() {
            items
        } else {
            fuzzy_filter(&items, &query, |item| match &item.model {
                Some(model) => get_model_search_candidates(&ModelSearchItem {
                    id: model.id.clone(),
                    provider: model.provider.clone(),
                    name: Some(model.name.clone()),
                }),
                None => vec![item.full_id.clone()],
            })
        };
        self.selected_index = self.selected_index.min(self.filtered_items.len().saturating_sub(1));
        self.update_list();
        let footer = self.get_footer_text();
        self.footer_text.set_text(footer);
    }

    fn notify_change(&self) {
        (self.callbacks.on_change)(self.enabled_ids.as_deref());
    }

    fn update_list(&mut self) {
        self.list_container.clear();

        if self.filtered_items.is_empty() {
            self.list_container.add_child(component(Text::new(
                self.theme.fg(ThemeColor::Muted, "  No matching models"),
                0,
                0,
            )));
            return;
        }

        let start_index = (self.selected_index as i64 - (self.max_visible / 2) as i64)
            .min(self.filtered_items.len() as i64 - self.max_visible as i64)
            .max(0) as usize;
        let end_index = (start_index + self.max_visible).min(self.filtered_items.len());
        let all_enabled = self.enabled_ids.is_none();

        for index in start_index..end_index {
            let item = &self.filtered_items[index];
            let is_selected = index == self.selected_index;
            let prefix = if is_selected {
                self.theme.fg(ThemeColor::Accent, "→ ")
            } else {
                "  ".to_string()
            };
            let id = item
                .model
                .as_ref()
                .map_or_else(|| item.full_id.clone(), |model| model.id.clone());
            let model_text = if is_selected {
                self.theme.fg(ThemeColor::Accent, &id)
            } else {
                id
            };
            let provider_badge = self.theme.fg(
                ThemeColor::Muted,
                &match &item.model {
                    Some(model) => format!(" [{}]", model.provider),
                    None => " [unavailable]".to_string(),
                },
            );
            let status = match &item.model {
                Some(_) if all_enabled => String::new(),
                Some(_) if item.enabled => self.theme.fg(ThemeColor::Success, " ✓"),
                _ => self.theme.fg(ThemeColor::Dim, " ✗"),
            };
            self.list_container.add_child(component(Text::new(
                format!("{prefix}{model_text}{provider_badge}{status}"),
                0,
                0,
            )));
        }

        if start_index > 0 || end_index < self.filtered_items.len() {
            self.list_container.add_child(component(Text::new(
                self.theme.fg(
                    ThemeColor::Muted,
                    &format!("  ({}/{})", self.selected_index + 1, self.filtered_items.len()),
                ),
                0,
                0,
            )));
        }

        if !self.filtered_items.is_empty() {
            let selected = &self.filtered_items[self.selected_index];
            self.list_container.add_child(component(Spacer::new(1)));
            let description = match &selected.model {
                Some(model) => format!("  Model Name: {}", model.name),
                None => "  Model unavailable".to_string(),
            };
            self.list_container.add_child(component(Text::new(
                self.theme.fg(ThemeColor::Muted, &description),
                0,
                0,
            )));
        }
    }

    fn current_full_id(&self) -> Option<String> {
        self.filtered_items
            .get(self.selected_index)
            .map(|item| item.full_id.clone())
    }

    fn reorder(&mut self, delta: i64) {
        let Some(enabled_ids) = &self.enabled_ids else {
            return;
        };
        let Some(item) = self.filtered_items.get(self.selected_index) else {
            return;
        };
        if !is_enabled(&self.enabled_ids, &item.full_id) {
            return;
        }
        let Some(current_index) = enabled_ids.iter().position(|id| *id == item.full_id) else {
            return;
        };
        let new_index = current_index as i64 + delta;
        if new_index < 0 || new_index >= enabled_ids.len() as i64 {
            return;
        }
        self.enabled_ids = move_enabled(self.enabled_ids.take(), &item.full_id, delta);
        self.is_dirty = true;
        self.selected_index = (self.selected_index as i64 + delta) as usize;
        self.refresh();
        self.notify_change();
    }

    fn toggle_selected(&mut self) {
        let Some(full_id) = self.current_full_id() else {
            return;
        };
        self.enabled_ids = toggle(self.enabled_ids.take(), &full_id);
        self.is_dirty = true;
        self.refresh();
        self.notify_change();
    }

    fn enable_all_filtered(&mut self) {
        let target_ids: Option<Vec<String>> = if self.search_input.get_value().is_empty() {
            None
        } else {
            Some(self.filtered_items.iter().map(|item| item.full_id.clone()).collect())
        };
        self.enabled_ids = enable_all(self.enabled_ids.take(), &self.all_ids, target_ids.as_deref());
        self.is_dirty = true;
        self.refresh();
        self.notify_change();
    }

    fn clear_all_filtered(&mut self) {
        let target_ids: Option<Vec<String>> = if self.search_input.get_value().is_empty() {
            None
        } else {
            Some(self.filtered_items.iter().map(|item| item.full_id.clone()).collect())
        };
        self.enabled_ids = clear_all(self.enabled_ids.take(), &self.all_ids, target_ids.as_deref());
        self.is_dirty = true;
        self.refresh();
        self.notify_change();
    }

    fn toggle_provider(&mut self) {
        let Some(item) = self.filtered_items.get(self.selected_index) else {
            return;
        };
        let Some(model) = &item.model else {
            return;
        };
        let provider = model.provider.clone();
        let provider_ids: Vec<String> = self
            .all_ids
            .iter()
            .filter(|id| {
                self.models_by_id
                    .get(*id)
                    .is_some_and(|model| model.provider == provider)
            })
            .cloned()
            .collect();
        let all_enabled = provider_ids.iter().all(|id| is_enabled(&self.enabled_ids, id));
        self.enabled_ids = if all_enabled {
            clear_all(self.enabled_ids.take(), &self.all_ids, Some(&provider_ids))
        } else {
            enable_all(self.enabled_ids.take(), &self.all_ids, Some(&provider_ids))
        };
        self.is_dirty = true;
        self.refresh();
        self.notify_change();
    }

    fn save(&mut self) {
        (self.callbacks.on_persist)(self.enabled_ids.as_deref());
        self.is_dirty = false;
        let footer = self.get_footer_text();
        self.footer_text.set_text(footer);
    }

    pub fn get_search_input(&self) -> &Input {
        &self.search_input
    }

    pub fn get_search_input_mut(&mut self) -> &mut Input {
        &mut self.search_input
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn enabled_model_ids(&self) -> Option<&[String]> {
        self.enabled_ids.as_deref()
    }
}

impl Focusable for ScopedModelsSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.search_input.set_focused(focused);
    }
}

impl Component for ScopedModelsSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.search_input.render(width));
        lines.extend(Spacer::new(1).render(width));
        lines.extend(self.list_container.render_children(width));
        lines.extend(Spacer::new(1).render(width));
        if let Some(refresh_status_text) = &mut self.refresh_status_text {
            lines.extend(refresh_status_text.render(width));
        }
        lines.extend(self.footer_text.render(width));
        lines.extend(dynamic_border(&self.theme).render(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "tui.select.up") {
            if self.filtered_items.is_empty() {
                return;
            }
            self.selected_index = if self.selected_index == 0 {
                self.filtered_items.len() - 1
            } else {
                self.selected_index - 1
            };
            self.update_list();
            return;
        }
        if keybindings.matches(data, "tui.select.down") {
            if self.filtered_items.is_empty() {
                return;
            }
            self.selected_index = if self.selected_index == self.filtered_items.len() - 1 {
                0
            } else {
                self.selected_index + 1
            };
            self.update_list();
            return;
        }

        let reorder_up = keybindings.matches(data, "app.models.reorderUp");
        let reorder_down = keybindings.matches(data, "app.models.reorderDown");
        if reorder_up || reorder_down {
            self.reorder(if reorder_up { -1 } else { 1 });
            return;
        }
        if keybindings.matches(data, "tui.select.confirm") {
            self.toggle_selected();
            return;
        }
        if keybindings.matches(data, "app.models.enableAll") {
            self.enable_all_filtered();
            return;
        }
        if keybindings.matches(data, "app.models.clearAll") {
            self.clear_all_filtered();
            return;
        }
        if keybindings.matches(data, "app.models.toggleProvider") {
            self.toggle_provider();
            return;
        }
        if keybindings.matches(data, "app.models.save") {
            self.save();
            return;
        }
        if matches_key(data, "ctrl+c") {
            if self.search_input.get_value().is_empty() {
                (self.callbacks.on_cancel)();
            } else {
                self.search_input.set_value("");
                self.refresh();
            }
            return;
        }
        if matches_key(data, "escape") {
            (self.callbacks.on_cancel)();
            return;
        }
        self.search_input.handle_input(data);
        self.refresh();
    }

    fn invalidate(&mut self) {
        self.top.invalidate_children();
        self.search_input.invalidate();
        self.list_container.invalidate_children();
        if let Some(refresh_status_text) = &mut self.refresh_status_text {
            refresh_status_text.invalidate();
        }
        self.footer_text.invalidate();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}
