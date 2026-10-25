//! Ported from `packages/coding-agent/src/modes/interactive/components/scoped-models-selector.ts`.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
    rc::Rc,
};

use pi_ai::types::Model;
use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::fuzzy::fuzzy_filter;
use pi_tui::keybindings::get_keybindings;
use pi_tui::keys::matches_key;
use pi_tui::tui::{Component, ComponentRef, Container, Focusable, component};

use super::super::model_search::{ModelSearchItem, get_model_search_candidates};
use super::super::theme::{ThemeColor, with_theme};
use super::dynamic_border::dynamic_border;
use super::keybinding_hints::key_text;

/// `null` = all enabled (no filter); otherwise an explicit ordered list.
pub type EnabledIds = Option<Vec<JsString>>;

fn is_enabled(enabled_ids: &EnabledIds, id: &JsString) -> bool {
    match enabled_ids {
        None => true,
        Some(ids) => ids.iter().any(|enabled| enabled == id),
    }
}

fn toggle(enabled_ids: &EnabledIds, id: &JsString) -> EnabledIds {
    let Some(ids) = enabled_ids else {
        return Some(vec![id.to_owned()]);
    };
    match ids.iter().position(|enabled| enabled == id) {
        Some(index) => {
            let mut result = ids.clone();
            result.remove(index);
            Some(result)
        }
        None => {
            let mut result = ids.clone();
            result.push(id.to_owned());
            Some(result)
        }
    }
}

fn enable_all(enabled_ids: &EnabledIds, all_ids: &[JsString], target_ids: Option<&[JsString]>) -> EnabledIds {
    let Some(ids) = enabled_ids else {
        return None;
    };
    let targets = target_ids.unwrap_or(all_ids);
    let mut result = ids.clone();
    for id in targets {
        if !result.contains(id) {
            result.push(id.clone());
        }
    }
    if result.len() == all_ids.len() {
        None
    } else {
        Some(result)
    }
}

fn clear_all(enabled_ids: &EnabledIds, all_ids: &[JsString], target_ids: Option<&[JsString]>) -> EnabledIds {
    let Some(ids) = enabled_ids else {
        return Some(match target_ids {
            Some(targets) => all_ids.iter().filter(|id| !targets.contains(id)).cloned().collect(),
            None => Vec::new(),
        });
    };
    let targets: HashSet<&JsString> = match target_ids {
        Some(targets) => targets.iter().collect(),
        None => ids.iter().collect(),
    };
    Some(ids.iter().filter(|id| !targets.contains(id)).cloned().collect())
}

fn move_id(enabled_ids: &EnabledIds, id: &JsString, delta: isize) -> EnabledIds {
    let Some(ids) = enabled_ids else {
        return None;
    };
    let mut list = ids.clone();
    let Some(index) = list.iter().position(|enabled| enabled == id) else {
        return Some(list);
    };
    let new_index = index as isize + delta;
    if new_index < 0 || new_index >= list.len() as isize {
        return Some(list);
    }
    list.swap(index, new_index as usize);
    Some(list)
}

fn get_sorted_ids(enabled_ids: &EnabledIds, all_ids: &[JsString]) -> Vec<JsString> {
    let Some(ids) = enabled_ids else {
        return all_ids.to_vec();
    };
    let enabled_set: HashSet<&JsString> = ids.iter().collect();
    let mut result = ids.clone();
    result.extend(all_ids.iter().filter(|id| !enabled_set.contains(id)).cloned());
    result
}

#[derive(Clone)]
struct ModelItem {
    full_id: JsString,
    model: Model,
    enabled: bool,
}

pub struct ModelsConfig {
    pub all_models: Vec<Model>,
    pub enabled_model_ids: EnabledIds,
}

pub struct ModelsCallbacks {
    /// Called whenever the enabled model set or order changes (session-only).
    pub on_change: Rc<dyn Fn(EnabledIds)>,
    /// Called when the user persists the current selection to settings.
    pub on_persist: Rc<dyn Fn(EnabledIds)>,
    pub on_cancel: Rc<dyn Fn()>,
}

/// Enable/disable and reorder models used for model cycling.
pub struct ScopedModelsSelectorComponent {
    models_by_id: HashMap<JsString, Model>,
    all_ids: Vec<JsString>,
    enabled_ids: EnabledIds,
    filtered_items: Vec<ModelItem>,
    selected_index: isize,
    container: Container,
    search_input: Rc<RefCell<Input>>,
    list_container: Rc<RefCell<Container>>,
    footer_text: Rc<RefCell<Text>>,
    callbacks: ModelsCallbacks,
    max_visible: usize,
    is_dirty: bool,
    focused: bool,
}

impl ScopedModelsSelectorComponent {
    pub fn new(config: ModelsConfig, callbacks: ModelsCallbacks) -> Self {
        let mut selector = Self {
            models_by_id: HashMap::new(),
            all_ids: Vec::new(),
            enabled_ids: config.enabled_model_ids,
            filtered_items: Vec::new(),
            selected_index: 0,
            container: Container::new(),
            search_input: Rc::new(RefCell::new(Input::new(InputOptions::default()))),
            list_container: Rc::new(RefCell::new(Container::new())),
            footer_text: Rc::new(RefCell::new(Text::new(JsString::default(), 0, 0))),
            callbacks,
            max_visible: 8,
            is_dirty: false,
            focused: false,
        };
        for model in config.all_models {
            let full_id = js_format!("{}/{}", model.provider, model.id);
            selector.models_by_id.insert(full_id.clone(), model);
            selector.all_ids.push(full_id);
        }
        selector.filtered_items = selector.build_items();

        let search_input: ComponentRef = selector.search_input.clone();
        let list_container: ComponentRef = selector.list_container.clone();
        let footer_text: ComponentRef = selector.footer_text.clone();
        selector.container = Container::with_children(vec![
            component(dynamic_border()),
            component(Spacer::new(1)),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, theme.bold("Model Configuration"))),
                0,
                0,
            )),
            component(Text::new(
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Muted,
                        &js_format!("Session-only. {} to save to settings.", key_text("app.models.save")),
                    )
                }),
                0,
                0,
            )),
            component(Spacer::new(1)),
            search_input,
            component(Spacer::new(1)),
            list_container,
            component(Spacer::new(1)),
            footer_text,
            component(dynamic_border()),
        ]);

        selector.set_footer_text(selector.get_footer_text());
        selector.update_list();
        selector
    }

    fn model_for(&self, id: &JsString) -> Option<&Model> {
        self.models_by_id.get(id)
    }

    fn build_items(&self) -> Vec<ModelItem> {
        get_sorted_ids(&self.enabled_ids, &self.all_ids)
            .into_iter()
            .filter_map(|id| {
                self.model_for(&id).cloned().map(|model| ModelItem {
                    model,
                    enabled: is_enabled(&self.enabled_ids, &id),
                    full_id: id,
                })
            })
            .collect()
    }

    fn get_footer_text(&self) -> JsString {
        let enabled_count = self.enabled_ids.as_ref().map_or(self.all_ids.len(), Vec::len);
        let all_enabled = self.enabled_ids.is_none();
        let count_text = if all_enabled {
            JsString::from("all enabled")
        } else {
            js_format!("{}/{} enabled", enabled_count, self.all_ids.len())
        };
        let parts = [
            js_format!("{} toggle", key_text("tui.select.confirm")),
            js_format!("{} all", key_text("app.models.enableAll")),
            js_format!("{} clear", key_text("app.models.clearAll")),
            js_format!("{} provider", key_text("app.models.toggleProvider")),
            js_format!(
                "{}/{} reorder",
                key_text("app.models.reorderUp"),
                key_text("app.models.reorderDown")
            ),
            js_format!("{} save", key_text("app.models.save")),
            count_text,
        ];
        let joined = JsString::join(parts, " · ");
        if self.is_dirty {
            with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("  {} ", joined)))
                + &with_theme(|theme| theme.fg(ThemeColor::Warning, "(unsaved)"))
        } else {
            with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("  {}", joined)))
        }
    }

    fn set_footer_text(&self, text: JsString) {
        self.footer_text.borrow_mut().set_text(text);
    }

    fn refresh(&mut self) {
        let query = self.search_input.borrow().get_value().clone();
        let items = self.build_items();
        self.filtered_items = if query.is_empty() {
            items
        } else {
            fuzzy_filter(&items, &query, |item| {
                get_model_search_candidates(&ModelSearchItem {
                    id: item.model.id.clone(),
                    provider: item.model.provider.clone(),
                    name: Some(item.model.name.clone()),
                })
            })
        };
        self.selected_index = self
            .selected_index
            .min(self.filtered_items.len().saturating_sub(1) as isize);
        self.update_list();
        self.set_footer_text(self.get_footer_text());
    }

    fn notify_change(&self) {
        (self.callbacks.on_change)(self.enabled_ids.clone());
    }

    fn update_list(&mut self) {
        let mut list_container = self.list_container.borrow_mut();
        list_container.clear();

        if self.filtered_items.is_empty() {
            list_container.add_child(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Muted, "  No matching models")),
                0,
                0,
            )));
            return;
        }

        let length = self.filtered_items.len();
        let start_index = (self.selected_index - (self.max_visible / 2) as isize)
            .min(length as isize - self.max_visible as isize)
            .max(0) as usize;
        let end_index = (start_index + self.max_visible).min(length);
        let all_enabled = self.enabled_ids.is_none();

        for index in start_index..end_index {
            let item = &self.filtered_items[index];
            let is_selected = index as isize == self.selected_index;
            let prefix = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "→ "))
            } else {
                JsString::from("  ")
            };
            let id = JsString::from(&item.model.id);
            let model_text = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, &id))
            } else {
                id
            };
            let badge_text = js_format!(" [{}]", item.model.provider);
            let provider_badge = with_theme(|theme| theme.fg(ThemeColor::Muted, &badge_text));
            let status = if all_enabled {
                JsString::default()
            } else if item.enabled {
                with_theme(|theme| theme.fg(ThemeColor::Success, " ✓"))
            } else {
                with_theme(|theme| theme.fg(ThemeColor::Dim, " ✗"))
            };
            list_container.add_child(component(Text::new(
                js_format!("{}{}{}{}", prefix, model_text, provider_badge, status),
                0,
                0,
            )));
        }

        if start_index > 0 || end_index < length {
            list_container.add_child(component(Text::new(
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Muted,
                        &js_format!("  ({}/{})", (self.selected_index + 1) as i64, length),
                    )
                }),
                0,
                0,
            )));
        }

        if !self.filtered_items.is_empty() {
            list_container.add_child(component(Spacer::new(1)));
            // Frozen filtered reorder can retain a negative index. Preserve its
            // explicit failure after the partial list update, not a clamp.
            let selected = self
                .filtered_items
                .get(self.selected_index as usize)
                .unwrap_or_else(|| {
                    crate::utils::uncaught::throw_uncaught(
                        "TypeError",
                        "Cannot read properties of undefined (reading 'model')",
                    )
                });
            let description = js_format!("  Model Name: {}", selected.model.name);
            list_container.add_child(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Muted, &description)),
                0,
                0,
            )));
        }
    }

    pub fn get_search_input(&self) -> Rc<RefCell<Input>> {
        Rc::clone(&self.search_input)
    }

    fn reorder(&mut self, delta: isize) {
        if self.enabled_ids.is_none() {
            return;
        }
        let Some(item) = self.filtered_items.get(self.selected_index as usize) else {
            return;
        };
        if !is_enabled(&self.enabled_ids, &item.full_id) {
            return;
        }
        let current_index = self
            .enabled_ids
            .as_ref()
            .and_then(|ids| ids.iter().position(|id| *id == item.full_id))
            .expect("enabled model is present in enabled IDs") as isize;
        let new_index = current_index + delta;
        let length = self.enabled_ids.as_ref().map_or(0, Vec::len) as isize;
        if new_index < 0 || new_index >= length {
            return;
        }
        let full_id = item.full_id.clone();
        self.enabled_ids = move_id(&self.enabled_ids, &full_id, delta);
        self.is_dirty = true;
        self.selected_index += delta;
        self.refresh();
        self.notify_change();
    }

    fn toggle_selected(&mut self) {
        let Some(item) = self.filtered_items.get(self.selected_index as usize) else {
            return;
        };
        let full_id = item.full_id.clone();
        self.enabled_ids = toggle(&self.enabled_ids, &full_id);
        self.is_dirty = true;
        self.refresh();
        self.notify_change();
    }

    fn target_ids(&self) -> Option<Vec<JsString>> {
        if self.search_input.borrow().get_value().is_empty() {
            None
        } else {
            Some(self.filtered_items.iter().map(|item| item.full_id.clone()).collect())
        }
    }

    fn toggle_provider(&mut self) {
        let Some(item) = self.filtered_items.get(self.selected_index as usize) else {
            return;
        };
        let provider = item.model.provider.clone();
        let provider_ids: Vec<JsString> = self
            .all_ids
            .iter()
            .filter(|id| self.model_for(id).is_some_and(|model| model.provider == provider))
            .cloned()
            .collect();
        let all_enabled = provider_ids.iter().all(|id| is_enabled(&self.enabled_ids, id));
        self.enabled_ids = if all_enabled {
            clear_all(&self.enabled_ids, &self.all_ids, Some(&provider_ids))
        } else {
            enable_all(&self.enabled_ids, &self.all_ids, Some(&provider_ids))
        };
        self.is_dirty = true;
        self.refresh();
        self.notify_change();
    }
}

impl Deref for ScopedModelsSelectorComponent {
    type Target = Container;
    fn deref(&self) -> &Self::Target {
        &self.container
    }
}
impl DerefMut for ScopedModelsSelectorComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.container
    }
}
impl Component for ScopedModelsSelectorComponent {
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        let keybindings = get_keybindings();

        if keybindings.matches(data, "tui.select.up") {
            if self.filtered_items.is_empty() {
                return;
            }
            self.selected_index = if self.selected_index == 0 {
                self.filtered_items.len() as isize - 1
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
            self.selected_index = if self.selected_index == self.filtered_items.len() as isize - 1 {
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
            let target_ids = self.target_ids();
            self.enabled_ids = enable_all(&self.enabled_ids, &self.all_ids, target_ids.as_deref());
            self.is_dirty = true;
            self.refresh();
            self.notify_change();
            return;
        }

        if keybindings.matches(data, "app.models.clearAll") {
            let target_ids = self.target_ids();
            self.enabled_ids = clear_all(&self.enabled_ids, &self.all_ids, target_ids.as_deref());
            self.is_dirty = true;
            self.refresh();
            self.notify_change();
            return;
        }

        if keybindings.matches(data, "app.models.toggleProvider") {
            self.toggle_provider();
            return;
        }

        if keybindings.matches(data, "app.models.save") {
            (self.callbacks.on_persist)(self.enabled_ids.clone());
            self.is_dirty = false;
            self.set_footer_text(self.get_footer_text());
            return;
        }

        if matches_key(data, "ctrl+c") {
            if !self.search_input.borrow().get_value().is_empty() {
                self.search_input.borrow_mut().set_value(JsString::default());
                self.refresh();
            } else {
                (self.callbacks.on_cancel)();
            }
            return;
        }

        if keybindings.matches(data, "tui.select.cancel") {
            (self.callbacks.on_cancel)();
            return;
        }

        self.search_input.borrow_mut().handle_input(data);
        self.refresh();
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}

impl Focusable for ScopedModelsSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.search_input.borrow_mut().set_focused(focused);
    }
}
